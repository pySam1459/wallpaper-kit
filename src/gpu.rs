use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HMODULE, S_OK};
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::core::{HRESULT, Interface};

use crate::{BLANK_RGBA, DeviceMode, Error, Size};

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
        })
    }

    pub(crate) fn render(&self, id: u64, size: Size) -> Result<Frame, Error> {
        let desc = texture_desc(size);
        let mut texture = None;
        let mut view = None;
        // SAFETY: descriptors and output pointers remain live. The worker alone
        // owns this context; the new resource is not published before completion.
        unsafe {
            self.device
                .CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.expect("CreateTexture2D supplied its output");
            self.device
                .CreateRenderTargetView(&texture, None, Some(&mut view))?;
            let view = view.expect("CreateRenderTargetView supplied its output");
            let mutex: IDXGIKeyedMutex = texture.cast()?;
            acquire(&mutex)?
                .then_some(())
                .ok_or_else(|| Error::Worker("new texture unexpectedly busy".into()))?;
            let color = BLANK_RGBA.map(|component| f32::from(component) / 255.0);
            self.context.ClearRenderTargetView(&view, &color);
            mutex.ReleaseSync(0)?;
            wait_for_gpu(&self.device, &self.context)?;
            let resource: IDXGIResource1 = texture.cast()?;
            let handle = resource.CreateSharedHandle(
                None,
                DXGI_SHARED_RESOURCE_READ.0 | DXGI_SHARED_RESOURCE_WRITE.0,
                None,
            )?;
            Ok(Frame {
                id,
                size,
                _texture: texture,
                handle,
                adapter: self.adapter.clone(),
                info: self.info.clone(),
            })
        }
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
/// Frames survive renderer resize and shutdown. The initial implementation
/// allocates on demand and never overwrites a published texture. Holding many
/// old frames consumes memory; a bounded reusable animation pool is future work.
pub struct Frame {
    id: u64,
    size: Size,
    _texture: ID3D11Texture2D,
    handle: HANDLE,
    adapter: IDXGIAdapter,
    info: AdapterInfo,
}

// SAFETY: the NT handle identifies an immutable shared GPU resource and closes
// only when its sole Frame owner drops. COM device/resource references support
// cross-thread use. No producer immediate context is exposed or shared.
unsafe impl Send for Frame {}
// SAFETY: concurrent consumers only read the immutable resource; keyed mutex
// acquisition serializes GPU copies. Each consumer uses its own context.
unsafe impl Sync for Frame {}

impl Frame {
    /// Request identity, including skipped/coalesced requests.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Physical dimensions.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Producer adapter identity.
    pub fn adapter_info(&self) -> &AdapterInfo {
        &self.info
    }

    /// Creates an independent same-adapter device and immediate context.
    /// The caller owns and serializes that context; creation is not a hot path.
    pub fn create_consumer_device(&self) -> Result<(ID3D11Device, ID3D11DeviceContext), Error> {
        create_device(Some(&self.adapter), D3D_DRIVER_TYPE_UNKNOWN)
    }

    /// Copies GPU-to-GPU into a matching consumer texture. Returns `false` if
    /// the shared resource is busy, without waiting for its keyed mutex.
    ///
    /// # Safety
    /// The calling thread must exclusively use `context` during this operation.
    /// `device`, `context`, and `destination` must belong to the same consumer
    /// device. Destination must have matching size/format/sample count, be a
    /// default-usage texture, and not be mapped. Keep the frame alive through
    /// submission. The producer never overwrites it; Direct3D retains submitted
    /// resource references until the GPU has finished using them.
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
            let texture: ID3D11Texture2D = device1.OpenSharedResource1(self.handle)?;
            let mutex: IDXGIKeyedMutex = texture.cast()?;
            if !acquire(&mutex)? {
                return Ok(false);
            }
            context.CopyResource(destination, &texture);
            mutex.ReleaseSync(0)?;
            Ok(true)
        }
    }

    /// Diagnostic readback on a separate device. May wait for the GPU for up
    /// to five seconds. Never use this method in the host's presentation path.
    pub fn readback(&self) -> Result<Pixels, Error> {
        let (device, context) = self.create_consumer_device()?;
        let device1: ID3D11Device1 = device.cast()?;
        // SAFETY: handle remains owned by self, and resource metadata matches.
        let texture: ID3D11Texture2D = unsafe { device1.OpenSharedResource1(self.handle)? };
        let mutex: IDXGIKeyedMutex = texture.cast()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !acquire(&mutex)? {
            if Instant::now() >= deadline {
                return Err(Error::Worker("capture synchronization timed out".into()));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let result = read_pixels(&device, &context, &texture, self.size);
        // SAFETY: this method acquired key 0; release also runs after read errors.
        unsafe {
            mutex.ReleaseSync(0)?;
        }
        result
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // SAFETY: this Frame uniquely owns its valid NT handle.
        unsafe {
            let _ = CloseHandle(self.handle);
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
            let result = (context.vtable().GetData)(
                context.as_raw(),
                query.as_raw(),
                std::ptr::null_mut(),
                0,
                D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
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
