//! Preserve the compositor's scRGB pixels on Advanced Color outputs.
use std::{sync::mpsc, time::Duration};
use windows::{
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem},
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
    },
    Win32::{
        Foundation::{E_FAIL, HMODULE, HWND},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::*,
            Dxgi::{Common::*, *},
            Gdi::HMONITOR,
        },
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
        },
    },
    core::{Error, Interface, Result, factory},
};

pub(crate) fn freeze(
    monitor: HMONITOR,
    window: HWND,
    width: u32,
    height: u32,
) -> Result<IDXGISwapChain1> {
    unsafe {
        let (mut device, mut context) = (None, None);
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
        let device = device.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context = context.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let dxgi: IDXGIDevice = device.cast()?;
        let direct3d: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&dxgi)?.cast()?;
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = interop.CreateForMonitor(monitor)?;
        let size = item.Size()?;
        if size.Width != width as i32 || size.Height != height as i32 {
            return Err(Error::new(E_FAIL, "Display size changed during capture"));
        }
        let factory: IDXGIFactory2 = dxgi.GetAdapter()?.GetParent()?;
        let swap = factory.CreateSwapChainForHwnd(
            &device,
            window,
            &DXGI_SWAP_CHAIN_DESC1 {
                Width: width,
                Height: height,
                Format: DXGI_FORMAT_R16G16B16A16_FLOAT,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                Scaling: DXGI_SCALING_NONE,
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                ..Default::default()
            },
            None,
            None,
        )?;
        swap.cast::<IDXGISwapChain3>()?
            .SetColorSpace1(DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709)?;
        let buffer: ID3D11Texture2D = swap.GetBuffer(0)?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &direct3d,
            DirectXPixelFormat::R16G16B16A16Float,
            1,
            size,
        )?;
        let session = pool.CreateCaptureSession(&item)?;
        session.SetIsCursorCaptureEnabled(false)?;
        let _ = session.SetIsBorderRequired(false);
        let (ready, frame_ready) = mpsc::sync_channel(1);
        let handler = TypedEventHandler::new(move |_, _| {
            let _ = ready.try_send(());
            Ok(())
        });
        let token = pool.FrameArrived(&handler)?;
        let captured = (|| {
            session.StartCapture()?;
            frame_ready
                .recv_timeout(Duration::from_millis(400))
                .map_err(|_| Error::new(E_FAIL, "No desktop frame arrived"))?;
            let frame = pool.TryGetNextFrame()?;
            let actual = frame.ContentSize()?;
            if actual.Width != size.Width || actual.Height != size.Height {
                return Err(Error::new(E_FAIL, "The display changed during capture"));
            }
            let surface: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
            let texture: ID3D11Texture2D = surface.GetInterface()?;
            context.CopyResource(&buffer, &texture);
            context.Flush();
            frame.Close()
        })();
        let _ = pool.RemoveFrameArrived(token);
        let _ = session.Close();
        let _ = pool.Close();
        captured?;
        Ok(swap)
    }
}
