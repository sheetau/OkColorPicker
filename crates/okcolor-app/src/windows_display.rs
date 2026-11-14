//! Read-only display capability discovery. A color space is not a monitor capability.
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::io;
use windows_sys::Win32::{
    Devices::Display::*,
    Foundation::{
        ERROR_CALL_NOT_IMPLEMENTED, ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_PARAMETER,
        ERROR_NOT_SUPPORTED, LUID, POINT, RECT,
    },
    Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromPoint,
    },
    UI::WindowsAndMessaging::GetWindowRect,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Sdr,
    WideColor,
    Hdr,
    /// The older API reports Advanced Color, but cannot distinguish WCG from HDR.
    AdvancedUnknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayColor {
    pub mode: ColorMode,
    pub bits_per_channel: u32,
    pub advanced_supported: bool,
    /// None when the older API cannot establish the luminance convention.
    pub reference_white_nits: Option<f32>,
}

impl DisplayColor {
    /// The linear scRGB scale for display-referred content, relative to 80 nits.
    pub fn white_scale(&self) -> Option<f32> {
        self.reference_white_nits.map(|nits| nits / 80.0)
    }
}

// windows-sys 0.61.2 predates this declaration. Layout and discriminant match
// Windows SDK 10.0.26100.0, wingdi.h DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO_2.
#[repr(C)]
#[derive(Default)]
struct AdvancedColorInfo2 {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    flags: u32,
    color_encoding: i32,
    bits_per_channel: u32,
    active_color_mode: i32,
}

pub fn query(window: &(impl HasWindowHandle + ?Sized)) -> io::Result<DisplayColor> {
    let handle = window.window_handle().map_err(io::Error::other)?;
    let RawWindowHandle::Win32(native) = handle.as_raw() else {
        return Err(io::Error::other("Expected a Win32 display window"));
    };
    // The center selects the principal display used by Advanced Color. Do not
    // assume the primary monitor or the GPU used for rendering owns this window.
    let mut rect = RECT::default();
    let mut monitor = MONITORINFOEXW::default();
    monitor.monitorInfo.cbSize = std::mem::size_of_val(&monitor) as u32;
    // SAFETY: the borrowed HWND remains live; all output buffers are correctly sized.
    unsafe {
        if GetWindowRect(native.hwnd.get() as _, &mut rect) == 0 {
            return Err(io::Error::last_os_error());
        }
        let center = POINT {
            x: ((i64::from(rect.left) + i64::from(rect.right)) / 2) as i32,
            y: ((i64::from(rect.top) + i64::from(rect.bottom)) / 2) as i32,
        };
        let target = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
        if GetMonitorInfoW(
            target,
            &mut monitor as *mut MONITORINFOEXW as *mut MONITORINFO,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    let mut matches = Vec::new();
    for path in active_paths()? {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: header(
                DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>(),
                path.sourceInfo.adapterId,
                path.sourceInfo.id,
            ),
            ..Default::default()
        };
        // SAFETY: header.size describes the containing, writable device-info structure.
        let result = unsafe { DisplayConfigGetDeviceInfo(&mut source.header) };
        if result == 0 && utf16_name(&source.viewGdiDeviceName) == utf16_name(&monitor.szDevice) {
            matches.push((path.targetInfo.adapterId, path.targetInfo.id));
        }
    }
    // Mirrored physical targets share one GDI name. Picking the first would claim
    // capabilities that might belong to a different panel.
    if matches.len() != 1 {
        return Err(io::Error::other(format!(
            "Expected one physical display target, found {}",
            matches.len()
        )));
    }
    let (adapter, id) = matches[0];
    query_target(adapter, id)
}

fn utf16_name(value: &[u16]) -> &[u16] {
    &value[..value.iter().position(|c| *c == 0).unwrap_or(value.len())]
}

fn header(kind: i32, size: usize, adapter: LUID, id: u32) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
    DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: kind,
        size: size as u32,
        adapterId: adapter,
        id,
    }
}

fn active_paths() -> io::Result<Vec<DISPLAYCONFIG_PATH_INFO>> {
    for _ in 0..3 {
        let mut paths_count = 0;
        let mut modes_count = 0;
        // SAFETY: count pointers and later arrays live throughout the synchronous calls.
        unsafe {
            check(GetDisplayConfigBufferSizes(
                QDC_ONLY_ACTIVE_PATHS,
                &mut paths_count,
                &mut modes_count,
            ))?;
            if paths_count > 4096 || modes_count > 16384 {
                return Err(io::Error::other(
                    "Display topology exceeds the supported query size",
                ));
            }
            let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); paths_count as usize];
            let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); modes_count as usize];
            let result = QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut paths_count,
                paths.as_mut_ptr(),
                &mut modes_count,
                modes.as_mut_ptr(),
                std::ptr::null_mut(),
            );
            if result == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            check(result)?;
            paths.truncate(paths_count as usize);
            return Ok(paths);
        }
    }
    Err(io::Error::other(
        "Display topology changed repeatedly during discovery",
    ))
}

fn query_target(adapter: LUID, id: u32) -> io::Result<DisplayColor> {
    let mut info = AdvancedColorInfo2 {
        header: header(15, std::mem::size_of::<AdvancedColorInfo2>(), adapter, id),
        ..Default::default()
    };
    // SAFETY: the SDK-compatible structure starts with the correctly sized header.
    let result = unsafe { DisplayConfigGetDeviceInfo(&mut info.header) } as u32;
    if result == 0 {
        let mode = decode_mode(info.flags, info.active_color_mode)?;
        let white = if mode == ColorMode::Hdr {
            let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL {
                header: header(
                    DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                    std::mem::size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>(),
                    adapter,
                    id,
                ),
                SDRWhiteLevel: 0,
            };
            // SAFETY: writable SDK structure, initialized header.
            check(unsafe { DisplayConfigGetDeviceInfo(&mut white.header) } as u32)?;
            Some(white_nits(white.SDRWhiteLevel)?)
        } else {
            Some(80.0)
        };
        return Ok(DisplayColor {
            mode,
            bits_per_channel: info.bits_per_channel,
            advanced_supported: info.flags & 1 != 0,
            reference_white_nits: white,
        });
    }
    if !matches!(
        result,
        ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED | ERROR_CALL_NOT_IMPLEMENTED
    ) {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let mut legacy = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO {
        header: header(
            DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
            std::mem::size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>(),
            adapter,
            id,
        ),
        ..Default::default()
    };
    // SAFETY: writable SDK structure; the union's value is the returned flag word.
    unsafe {
        check(DisplayConfigGetDeviceInfo(&mut legacy.header) as u32)?;
        let flags = legacy.Anonymous.value;
        let mode = if flags & 2 != 0 {
            ColorMode::AdvancedUnknown
        } else {
            ColorMode::Sdr
        };
        Ok(DisplayColor {
            mode,
            bits_per_channel: legacy.bitsPerColorChannel,
            advanced_supported: flags & 1 != 0,
            reference_white_nits: (mode == ColorMode::Sdr).then_some(80.0),
        })
    }
}

fn check(code: u32) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code as i32))
    }
}

fn decode_mode(flags: u32, mode: i32) -> io::Result<ColorMode> {
    match (flags & 2 != 0, mode) {
        (false, 0) => Ok(ColorMode::Sdr),
        (true, 1) => Ok(ColorMode::WideColor),
        (true, 2) => Ok(ColorMode::Hdr),
        _ => Err(io::Error::other(
            "Unknown or inconsistent Advanced Color mode",
        )),
    }
}

fn white_nits(value: u32) -> io::Result<f32> {
    if value == 0 {
        return Err(io::Error::other("Invalid zero SDR reference white level"));
    }
    Ok(value as f32 * 80.0 / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_color_mode_is_not_inferred_from_supported_or_user_enabled_flags() {
        assert_eq!(
            decode_mode(1 | 16 | 32 | 64 | 128, 0).unwrap(),
            ColorMode::Sdr
        );
        assert_eq!(decode_mode(3 | 64, 1).unwrap(), ColorMode::WideColor);
        assert_eq!(decode_mode(3 | 16, 2).unwrap(), ColorMode::Hdr);
        for (flags, mode) in [(1, 2), (3, 0), (3, 9)] {
            assert!(decode_mode(flags, mode).is_err());
        }
        assert_eq!(white_nits(1000).unwrap(), 80.0);
        assert_eq!(white_nits(2500).unwrap(), 200.0);
        assert!(white_nits(0).is_err());
        assert_eq!(std::mem::size_of::<AdvancedColorInfo2>(), 36);
        assert_eq!(
            std::mem::offset_of!(AdvancedColorInfo2, active_color_mode),
            32
        );
    }
}
