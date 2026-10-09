// Native presentation boundary. Each unsafe operation documents its safety contract.
#![allow(unsafe_code)]

use super::support::Result;
use scene_renderer::{Frame, Pixels, Size};
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
