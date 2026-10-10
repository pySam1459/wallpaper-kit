// Native presentation boundary. Each unsafe operation documents its safety contract.
#![allow(unsafe_code)]

use super::support::Result;
use wallpaper_kit::{Frame, Pixels, Size};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::core::Interface;
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

pub struct Presentation {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap: IDXGISwapChain1,
    size: Size,
}

impl Presentation {
    pub fn new(window: &Window, frame: &Frame) -> Result<Self> {
        let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
            return Err("viewer requires a Windows window".into());
        };
        let (device, context) = frame.create_consumer_device()?;
        // SAFETY: live device; window remains owned by the App until this
        // presentation is dropped. Factory creates owned COM references.
        let swap = unsafe {
            let adapter = device.cast::<IDXGIDevice>()?.GetAdapter()?;
            let factory: IDXGIFactory2 = adapter.GetParent()?;
            let size = frame.size();
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: size.width(),
                Height: size.height(),
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                ..Default::default()
            };
            factory.CreateSwapChainForHwnd(
                &device,
                HWND(handle.hwnd.get() as *mut _),
                &desc,
                None,
                None,
            )?
        };
        Ok(Self {
            device,
            context,
            swap,
            size: frame.size(),
        })
    }

    pub fn draw(&mut self, frame: &Frame) -> Result<bool> {
        // SAFETY: this thread exclusively owns its consumer context and swap
        // chain. GetBuffer references do not survive draw/resize calls.
        unsafe {
            if self.size != frame.size() {
                self.swap.ResizeBuffers(
                    0,
                    frame.size().width(),
                    frame.size().height(),
                    DXGI_FORMAT_UNKNOWN,
                    DXGI_SWAP_CHAIN_FLAG(0),
                )?;
                self.size = frame.size();
            }
            let target: ID3D11Texture2D = self.swap.GetBuffer(0)?;
            if !frame.copy_to(&self.device, &self.context, &target)? {
                return Ok(false);
            }
            let result = self.swap.Present(0, DXGI_PRESENT_DO_NOT_WAIT);
            if result == DXGI_ERROR_WAS_STILL_DRAWING {
                return Ok(false);
            }
            result.ok()?;
            Ok(true)
        }
    }

    // Capture the imported frame after copying into the real consumer back
    // buffer. This diagnostic path may wait/read back; normal draw does neither.
    pub fn capture(&mut self, frame: &Frame) -> Result<Pixels> {
        // SAFETY: matching live resources and exclusive local context. Map
        // remains active until all pitched rows are copied and then unmapped.
        unsafe {
            let target: ID3D11Texture2D = self.swap.GetBuffer(0)?;
            if !frame.copy_to(&self.device, &self.context, &target)? {
                return Err("frame is busy; retry capture".into());
            }
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            target.GetDesc(&mut desc);
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.MiscFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            self.device
                .CreateTexture2D(&desc, None, Some(&mut staging))?;
            let staging = staging.ok_or("no staging output")?;
            self.context.CopyResource(&staging, &target);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let row_bytes = self.size.width() as usize * 4;
            let mut rgba = vec![0; row_bytes * self.size.height() as usize];
            for (row, bytes) in rgba.chunks_exact_mut(row_bytes).enumerate() {
                bytes.copy_from_slice(std::slice::from_raw_parts(
                    (mapped.pData as *const u8).add(row * mapped.RowPitch as usize),
                    row_bytes,
                ));
            }
            self.context.Unmap(&staging, 0);
            Ok(Pixels {
                size: self.size,
                rgba,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    use wallpaper_kit::{DeviceMode, Renderer};

    fn wait_frame(renderer: &Renderer, id: u64) -> Arc<Frame> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(frame) = renderer.try_frame().unwrap()
                && frame.id() == id
            {
                return frame;
            }
            assert!(Instant::now() < deadline, "queued-copy fixture timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn target(device: &ID3D11Device, size: Size) -> ID3D11Texture2D {
        let mut target = None;
        let desc = D3D11_TEXTURE2D_DESC {
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
            ..Default::default()
        };
        // SAFETY: live device, matching validated dimensions and owned output.
        unsafe {
            device
                .CreateTexture2D(&desc, None, Some(&mut target))
                .unwrap();
        }
        target.unwrap()
    }

    fn read_target(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        source: &ID3D11Texture2D,
        size: Size,
    ) -> Pixels {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        let mut staging = None;
        // SAFETY: test thread exclusively owns this context. Source belongs to
        // the same device. Map waits for queued copies and remains live until
        // all pitched rows have been read and Unmap is called.
        unsafe {
            source.GetDesc(&mut desc);
            desc.Usage = D3D11_USAGE_STAGING;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            device
                .CreateTexture2D(&desc, None, Some(&mut staging))
                .unwrap();
            let staging = staging.unwrap();
            context.CopyResource(&staging, source);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .unwrap();
            let row_bytes = size.width() as usize * 4;
            let mut rgba = vec![0; row_bytes * size.height() as usize];
            for (row, bytes) in rgba.chunks_exact_mut(row_bytes).enumerate() {
                bytes.copy_from_slice(std::slice::from_raw_parts(
                    (mapped.pData as *const u8).add(row * mapped.RowPitch as usize),
                    row_bytes,
                ));
            }
            context.Unmap(&staging, 0);
            Pixels { size, rgba }
        }
    }

    #[test]
    fn queued_consumer_copies_survive_source_release_and_reuse() {
        let size = Size::new(320, 180).unwrap();
        let (renderer, _events) = Renderer::new(size, DeviceMode::Software).unwrap();
        let first = wait_frame(&renderer, 1);
        let (device, context) = first.create_consumer_device().unwrap();
        drop(first);
        let targets: Vec<_> = (0..3).map(|_| target(&device, size)).collect();
        for batch in 0..30 {
            for (index, target) in targets.iter().enumerate() {
                let time = (batch * 3 + index) as u64 * 50;
                let id = renderer
                    .render_fixture_at(Duration::from_millis(time))
                    .unwrap();
                let frame = wait_frame(&renderer, id);
                // SAFETY: exclusive context and same-device matching target.
                // Release the source lease immediately after submission, with
                // no readback wait before scheduling subsequent producer work.
                assert!(unsafe { frame.copy_to(&device, &context, target).unwrap() });
                drop(frame);
            }
            for (index, target) in targets.iter().enumerate() {
                let time = (batch * 3 + index) as u64 * 50;
                super::super::motion_checks::check_pixels(
                    &read_target(&device, &context, target, size),
                    time,
                )
                .unwrap();
            }
        }
        renderer.shutdown().unwrap();
    }
}
