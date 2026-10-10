// Native graphics boundary. Each unsafe operation documents its safety contract.
#![allow(unsafe_code)]

use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HMODULE, RECT, S_FALSE, S_OK};
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::core::{HRESULT, Interface};

use crate::{
    BLANK_RGBA, Content, DeviceMode, Error, OutputStats, Request, Shared, Size, TextureFormat,
    TexturePage, motion,
};

mod image;

/// Adapter identity used by the producer. Adapter LUID is local to this system.
#[derive(Clone, Debug)]
pub struct AdapterInfo {
    /// Driver-supplied adapter description.
    pub name: String,
    /// PCI vendor identity, where available.
    pub vendor_id: u32,
    /// PCI device identity, where available.
    pub device_id: u32,
    /// Low and high components of the adapter LUID.
    pub luid: (u32, i32),
    /// Whether WARP was explicitly selected.
    pub software: bool,
}

pub(crate) struct Gpu {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    adapter: IDXGIAdapter,
    info: AdapterInfo,
    slots: Vec<Slot>,
    allocations: u64,
    submissions: u64,
    image: Option<image::ImageRenderer>,
}

struct Slot {
    output: Arc<Output>,
    state: SlotState,
}

enum SlotState {
    Available,
    Submitted { request: Request, started: Instant },
    Leased(Weak<Frame>),
}

pub(crate) enum Submission {
    Submitted,
    Leased,
    Busy,
}

struct Output {
    size: Size,
    _texture: ID3D11Texture2D,
    view: ID3D11RenderTargetView,
    mutex: IDXGIKeyedMutex,
    query: ID3D11Query,
    handle: HANDLE,
    adapter: IDXGIAdapter,
    info: AdapterInfo,
}

// SAFETY: Output uniquely owns the shared handle. Its COM references support
// cross-thread use; no immediate context is stored here. The worker writes only
// after every Frame lease is gone and it has acquired exclusive GPU access.
unsafe impl Send for Output {}
// SAFETY: Frame consumers only read under keyed-mutex synchronization. Worker
// writes require the absence of all Frame leases, so they cannot overlap reads.
unsafe impl Sync for Output {}

impl Drop for Output {
    fn drop(&mut self) {
        // SAFETY: this Output uniquely owns its valid NT handle.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

impl Gpu {
    pub(crate) fn new(mode: DeviceMode) -> Result<Self, Error> {
        let driver = match mode {
            DeviceMode::Hardware => D3D_DRIVER_TYPE_HARDWARE,
            DeviceMode::Software => D3D_DRIVER_TYPE_WARP,
        };
        let (device, context) = create_device(None, driver)?;
        // SAFETY: device is live; these calls return owned COM references.
        let adapter = unsafe { device.cast::<IDXGIDevice>()?.GetAdapter()? };
        // SAFETY: adapter is a live COM object and GetDesc has no borrowed output.
        let desc = unsafe { adapter.GetDesc()? };
        let end = desc
            .Description
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(desc.Description.len());
        let info = AdapterInfo {
            name: String::from_utf16_lossy(&desc.Description[..end]),
            vendor_id: desc.VendorId,
            device_id: desc.DeviceId,
            luid: (desc.AdapterLuid.LowPart, desc.AdapterLuid.HighPart),
            software: matches!(mode, DeviceMode::Software),
        };
        Ok(Self {
            device,
            context,
            adapter,
            info,
            slots: Vec::new(),
            allocations: 0,
            submissions: 0,
            image: None,
        })
    }

    fn create_output(&mut self, size: Size) -> Result<Arc<Output>, Error> {
        let desc = texture_desc(size);
        let mut texture = None;
        let mut view = None;
        let mut query = None;
        // SAFETY: descriptors and output pointers remain live; all results are
        // owned COM references. No resource is published before GPU completion.
        unsafe {
            self.device
                .CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.expect("CreateTexture2D supplied its output");
            self.device
                .CreateRenderTargetView(&texture, None, Some(&mut view))?;
            let view = view.expect("CreateRenderTargetView supplied its output");
            let mutex: IDXGIKeyedMutex = texture.cast()?;
            self.device.CreateQuery(
                &D3D11_QUERY_DESC {
                    Query: D3D11_QUERY_EVENT,
                    MiscFlags: 0,
                },
                Some(&mut query),
            )?;
            let resource: IDXGIResource1 = texture.cast()?;
            let handle = resource.CreateSharedHandle(
                None,
                DXGI_SHARED_RESOURCE_READ.0 | DXGI_SHARED_RESOURCE_WRITE.0,
                None,
            )?;
            self.allocations += 1;
            Ok(Arc::new(Output {
                size,
                _texture: texture,
                view,
                mutex,
                query: query.expect("query creation supplied its output"),
                handle,
                adapter: self.adapter.clone(),
                info: self.info.clone(),
            }))
        }
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.slots
            .iter()
            .any(|slot| matches!(slot.state, SlotState::Submitted { .. }))
    }

    pub(crate) fn stats(&self) -> OutputStats {
        OutputStats {
            resident_textures: self.slots.len(),
            resident_bytes: self
                .slots
                .iter()
                .map(|slot| {
                    u64::from(slot.output.size.width()) * u64::from(slot.output.size.height()) * 4
                })
                .sum(),
            texture_allocations: self.allocations,
            submitted_frames: self.submissions,
            source_texture_bytes: self.image.as_ref().map_or(0, |image| image.bytes()),
            effect_texture_bytes: self.image.as_ref().map_or(0, |image| image.effect_bytes()),
            texture_uploads: self.image.as_ref().map_or(0, |image| image.uploads),
            ..Default::default()
        }
    }

    pub(crate) fn try_submit(&mut self, request: &Request) -> Result<Submission, Error> {
        debug_assert!(!self.has_pending());
        let mut selected = None;
        let mut busy = false;
        // Prefer existing matching textures. Retired sizes still occupy a slot
        // while leased; replace them only after GPU access has returned.
        for matching in [true, false] {
            for (index, slot) in self.slots.iter_mut().enumerate() {
                if let SlotState::Leased(frame) = &slot.state
                    && frame.strong_count() == 0
                {
                    slot.state = SlotState::Available;
                }
                if matches!(slot.state, SlotState::Available)
                    && (slot.output.size == request.size) == matching
                {
                    if acquire(&slot.output.mutex)? {
                        selected = Some(index);
                        break;
                    }
                    busy = true;
                }
            }
            if selected.is_some() {
                break;
            }
        }
        let index = if let Some(index) = selected {
            if self.slots[index].output.size != request.size {
                // SAFETY: the worker just acquired this resource's key 0.
                unsafe {
                    self.slots[index].output.mutex.ReleaseSync(0)?;
                }
                self.slots.remove(index);
                let output = self.create_output(request.size)?;
                self.slots.push(Slot {
                    output,
                    state: SlotState::Available,
                });
                let index = self.slots.len() - 1;
                if !acquire(&self.slots[index].output.mutex)? {
                    return Err(Error::Worker("new output texture unexpectedly busy".into()));
                }
                index
            } else {
                index
            }
        } else if self.slots.len() < 3 {
            let output = self.create_output(request.size)?;
            self.slots.push(Slot {
                output,
                state: SlotState::Available,
            });
            let index = self.slots.len() - 1;
            if !acquire(&self.slots[index].output.mutex)? {
                return Err(Error::Worker("new output texture unexpectedly busy".into()));
            }
            index
        } else {
            return Ok(if busy {
                Submission::Busy
            } else {
                Submission::Leased
            });
        };
        let pages = match &request.content {
            Content::Images(composition) => Some(composition.pages()),
            _ => request
                .content
                .texture()
                .map(|(page, _, _)| std::slice::from_ref(page)),
        };
        if let Some(pages) = pages {
            // Release GPU access during fallible compilation/upload. No Frame
            // can appear for this available slot while the worker prepares it.
            // SAFETY: slot selection acquired this output's key 0.
            unsafe {
                self.slots[index].output.mutex.ReleaseSync(0)?;
            }
            if self.image.is_none() {
                self.image = Some(image::ImageRenderer::new(&self.device)?);
            }
            self.image
                .as_mut()
                .expect("image pipeline initialized")
                .prepare(
                    &self.device,
                    pages,
                    match &request.content {
                        Content::Images(composition) => composition.blur_extent(),
                        _ => None,
                    },
                )?;
            if !acquire(&self.slots[index].output.mutex)? {
                return Ok(Submission::Busy);
            }
        } else if let Some(image) = &mut self.image {
            image.release_texture();
        }
        let slot = &mut self.slots[index];
        let output = &slot.output;
        // Prepare fallible interface access before drawing. Always release the
        // acquired mutex, including when interface lookup fails.
        let context1 = match &request.content {
            Content::MotionFixture(_) => self.context.cast::<ID3D11DeviceContext1>().map(Some),
            Content::Blank
            | Content::TexturePage(_)
            | Content::TextureRegion(_)
            | Content::Images(_) => Ok(None),
        };
        let context1 = match context1 {
            Ok(context) => context,
            Err(error) => {
                // SAFETY: this worker acquired output key 0 above.
                unsafe {
                    output.mutex.ReleaseSync(0)?;
                }
                return Err(Error::Graphics(error));
            }
        };
        // SAFETY: exclusive worker context, no Frame leases, and acquired GPU
        // access. Query and views are owned by the slot through completion.
        unsafe {
            self.context.ClearRenderTargetView(
                &output.view,
                &BLANK_RGBA.map(|component| f32::from(component) / 255.0),
            );
            if let Content::MotionFixture(time) = &request.content {
                let [left, top, right, bottom] = motion::rectangle(request.size, *time);
                context1
                    .expect("motion fixture requires context1")
                    .ClearView(
                        &output.view,
                        &[224.0 / 255.0, 160.0 / 255.0, 64.0 / 255.0, 1.0],
                        Some(&[RECT {
                            left,
                            top,
                            right,
                            bottom,
                        }]),
                    );
            }
            if let Some((page, origin, extent)) = request.content.texture() {
                self.image.as_ref().expect("image prepared").draw(
                    &self.context,
                    output,
                    page.format(),
                    origin,
                    extent,
                );
            }
            if let Content::Images(composition) = &request.content {
                self.image.as_ref().expect("image prepared").draw_layers(
                    &self.context,
                    output,
                    composition,
                );
            }
            output.mutex.ReleaseSync(0)?;
            self.context.End(&output.query);
            self.context.Flush();
        }
        slot.state = SlotState::Submitted {
            request: request.clone(),
            started: Instant::now(),
        };
        self.submissions += 1;
        Ok(Submission::Submitted)
    }

    pub(crate) fn poll(
        &mut self,
        desired: u64,
        sequence: u64,
        shared: Weak<Shared>,
    ) -> Result<Option<Arc<Frame>>, Error> {
        for slot in &mut self.slots {
            let SlotState::Submitted { request, started } = &slot.state else {
                continue;
            };
            // SAFETY: exclusive worker immediate context; live query. Status
            // only, so null output and size 0 are required. Flags 0 permit
            // submission during this single poll; no completion wait loop.
            let result = unsafe {
                (self.context.vtable().GetData)(
                    self.context.as_raw(),
                    slot.output.query.as_raw(),
                    std::ptr::null_mut(),
                    0,
                    0,
                )
            };
            if result == S_FALSE {
                if started.elapsed() >= Duration::from_secs(5) {
                    return Err(Error::Worker("GPU completion timed out".into()));
                }
                return Ok(None);
            }
            if result != S_OK {
                return Err(Error::Graphics(windows::core::Error::from_hresult(result)));
            }
            if request.id != desired {
                slot.state = SlotState::Available;
                return Ok(None);
            }
            let frame = Arc::new(Frame {
                id: request.id,
                sequence,
                output: slot.output.clone(),
                shared,
                fixture_time: match &request.content {
                    Content::Blank
                    | Content::TexturePage(_)
                    | Content::TextureRegion(_)
                    | Content::Images(_) => None,
                    Content::MotionFixture(time) => Some(*time),
                },
                texture_format: request.content.texture().map(|(page, _, _)| page.format()),
                texture_region: request
                    .content
                    .texture()
                    .map(|(_, origin, extent)| (origin, extent)),
                image_layer_count: match &request.content {
                    Content::Images(composition) => Some(composition.layers().len()),
                    _ => None,
                },
            });
            slot.state = SlotState::Leased(Arc::downgrade(&frame));
            return Ok(Some(frame));
        }
        Ok(None)
    }
}

fn texture_desc(size: Size) -> D3D11_TEXTURE2D_DESC {
    D3D11_TEXTURE2D_DESC {
        Width: size.width(),
        Height: size.height(),
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
        MiscFlags: (D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0 | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0)
            as u32,
        ..Default::default()
    }
}

fn create_device(
    adapter: Option<&IDXGIAdapter>,
    driver: D3D_DRIVER_TYPE,
) -> Result<(ID3D11Device, ID3D11DeviceContext), Error> {
    let mut device = None;
    let mut context = None;
    // SAFETY: all outputs and the optional adapter are live for this call.
    unsafe {
        D3D11CreateDevice(
            adapter,
            driver,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }
    Ok((
        device.expect("device creation supplied a device"),
        context.expect("device creation supplied a context"),
    ))
}

/// An immutable, completed RGBA8 GPU frame. Hold it while using its resource.
///
/// Frames survive renderer resize and shutdown. Their leases prevent reuse.
/// Holding all three output slots delays new output until a frame is released.
pub struct Frame {
    id: u64,
    sequence: u64,
    output: Arc<Output>,
    shared: Weak<Shared>,
    fixture_time: Option<Duration>,
    texture_format: Option<TextureFormat>,
    texture_region: Option<([u32; 2], [u32; 2])>,
    image_layer_count: Option<usize>,
}

impl Frame {
    /// Number of ordered image draws, or `None` for other diagnostics.
    /// This metadata does not retain CPU source pages or composition objects.
    pub fn image_layer_count(&self) -> Option<usize> {
        self.image_layer_count
    }
    /// Explicit fixture playback time, or `None` for blank/texture output.
    pub fn fixture_time(&self) -> Option<Duration> {
        self.fixture_time
    }

    /// Source storage of a diagnostic page/region draw, or `None` otherwise.
    pub fn texture_format(&self) -> Option<TextureFormat> {
        self.texture_format
    }

    /// Source pixel origin and extent of a diagnostic page/region draw.
    /// Full-page draws have origin `[0,0]`. No CPU source data is held by Frame.
    pub fn texture_region(&self) -> Option<([u32; 2], [u32; 2])> {
        self.texture_region
    }

    /// Request identity, including skipped/coalesced requests.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Monotonic publication sequence, independent of coalesced request IDs.
    /// Starts at one; superseded renders do not consume a sequence number.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Physical dimensions.
    pub fn size(&self) -> Size {
        self.output.size
    }

    /// Producer adapter identity.
    pub fn adapter_info(&self) -> &AdapterInfo {
        &self.output.info
    }

    /// Creates an independent same-adapter device and immediate context.
    /// The caller owns and serializes that context; creation is not a hot path.
    pub fn create_consumer_device(&self) -> Result<(ID3D11Device, ID3D11DeviceContext), Error> {
        create_device(Some(&self.output.adapter), D3D_DRIVER_TYPE_UNKNOWN)
    }

    /// Copies GPU-to-GPU into a matching consumer texture. Returns `false` if
    /// the shared resource is busy, without waiting for its keyed mutex.
    ///
    /// # Safety
    /// The calling thread must exclusively use `context` during this operation.
    /// `device`, `context`, and `destination` must belong to the same consumer
    /// device. Destination must have matching size/format/sample count, be a
    /// default-usage texture, and not be mapped. Keep the frame alive through
    /// submission. GPU work is submitted before returning. The producer waits
    /// for all Frame leases to end and for keyed-mutex access before reuse;
    /// dropping this frame after submission does not permit an unsynchronized
    /// overwrite. This method does not wait for copy completion.
    pub unsafe fn copy_to(
        &self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        destination: &ID3D11Texture2D,
    ) -> Result<bool, Error> {
        // SAFETY: caller meets context/resource requirements. Imported resources
        // remain owned locally until after CopyResource and ReleaseSync.
        unsafe {
            let device1: ID3D11Device1 = device.cast()?;
            let texture: ID3D11Texture2D = device1.OpenSharedResource1(self.output.handle)?;
            let mutex: IDXGIKeyedMutex = texture.cast()?;
            if !acquire(&mutex)? {
                return Ok(false);
            }
            context.CopyResource(destination, &texture);
            mutex.ReleaseSync(0)?;
            context.Flush();
            Ok(true)
        }
    }

    /// Diagnostic readback on a separate device. May wait for the GPU for up
    /// to five seconds. Never use this method in the host's presentation path.
    pub fn readback(&self) -> Result<Pixels, Error> {
        let (device, context) = self.create_consumer_device()?;
        let device1: ID3D11Device1 = device.cast()?;
        // SAFETY: handle remains owned by self, and resource metadata matches.
        let texture: ID3D11Texture2D = unsafe { device1.OpenSharedResource1(self.output.handle)? };
        let mutex: IDXGIKeyedMutex = texture.cast()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !acquire(&mutex)? {
            if Instant::now() >= deadline {
                return Err(Error::Worker("capture synchronization timed out".into()));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let result = read_pixels(&device, &context, &texture, self.size());
        // SAFETY: this method acquired key 0; release also runs after read errors.
        unsafe {
            mutex.ReleaseSync(0)?;
        }
        result
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.frame_released();
        }
    }
}

/// Tightly packed RGBA8 pixels from an explicit diagnostic capture.
pub struct Pixels {
    /// Physical dimensions.
    pub size: Size,
    /// Rows in top-to-bottom order, with no row padding.
    pub rgba: Vec<u8>,
}

fn acquire(mutex: &IDXGIKeyedMutex) -> Result<bool, Error> {
    // SAFETY: live COM object, immediate-return timeout. Call the vtable to
    // distinguish WAIT_TIMEOUT/WAIT_ABANDONED, which are positive HRESULTs and
    // would incorrectly become Ok through the generated .ok() wrapper.
    let result = unsafe { (mutex.vtable().AcquireSync)(mutex.as_raw(), 0, 0) };
    match result {
        S_OK => Ok(true),
        HRESULT(258) => Ok(false),
        HRESULT(128) => Err(Error::Worker(
            "shared output keyed mutex was abandoned".into(),
        )),
        _ => Err(Error::Graphics(windows::core::Error::from_hresult(result))),
    }
}

fn wait_for_gpu(device: &ID3D11Device, context: &ID3D11DeviceContext) -> Result<(), Error> {
    let mut query = None;
    let desc = D3D11_QUERY_DESC {
        Query: D3D11_QUERY_EVENT,
        MiscFlags: 0,
    };
    // SAFETY: context is exclusively owned on this thread; query outputs live.
    unsafe {
        device.CreateQuery(&desc, Some(&mut query))?;
        let query = query.expect("query creation supplied its output");
        context.End(&query);
        context.Flush();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            // Permit query submission during polling. DONOTFLUSH left this
            // query pending for ~390 ms per frame on the tested NVIDIA adapter,
            // even after the explicit Flush above. This wait stays on the worker
            // or diagnostic readback thread, never in try_frame().
            let result = (context.vtable().GetData)(
                context.as_raw(),
                query.as_raw(),
                std::ptr::null_mut(),
                0,
                0,
            );
            if result == S_OK {
                return Ok(());
            }
            if result.is_err() {
                return Err(Error::Graphics(windows::core::Error::from_hresult(result)));
            }
            if Instant::now() >= deadline {
                return Err(Error::Worker("GPU completion timed out".into()));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

fn read_pixels(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    texture: &ID3D11Texture2D,
    size: Size,
) -> Result<Pixels, Error> {
    let mut desc = texture_desc(size);
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.MiscFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
    let mut staging = None;
    // SAFETY: context is local to this operation. Sizes are validated; Map's
    // rows remain readable until Unmap, and RowPitch is honored when copying.
    unsafe {
        device.CreateTexture2D(&desc, None, Some(&mut staging))?;
        let staging = staging.expect("staging creation supplied its output");
        context.CopyResource(&staging, texture);
        wait_for_gpu(device, context)?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        let row_bytes = size.width() as usize * 4;
        let mut rgba = vec![0; row_bytes * size.height() as usize];
        for (row, target) in rgba.chunks_exact_mut(row_bytes).enumerate() {
            let source = (mapped.pData as *const u8).add(row * mapped.RowPitch as usize);
            target.copy_from_slice(std::slice::from_raw_parts(source, row_bytes));
        }
        context.Unmap(&staging, 0);
        Ok(Pixels { size, rgba })
    }
}
