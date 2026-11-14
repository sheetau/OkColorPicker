//! In-memory QR detection. Neither pixels nor decoded credentials are logged.
use crate::Pairing;
use std::fmt;

pub const MAX_PIXELS: usize = 16_777_216;
const MAX_DIMENSION: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanError {
    InvalidImage,
    NoPairing,
    Ambiguous,
    TooManyCodes,
    CaptureUnavailable,
    Cancelled,
}
impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidImage => "Screen image exceeds the scan limits",
            Self::NoPairing => "Show the CSP companion QR on screen, then scan again",
            Self::Ambiguous => "Multiple CSP pairing codes found; leave only one visible",
            Self::TooManyCodes => "Too many QR codes on screen; leave only the CSP code visible",
            Self::CaptureUnavailable => "Screen capture is unavailable",
            Self::Cancelled => "CSP scan cancelled",
        })
    }
}
impl std::error::Error for ScanError {}

pub fn pixel_count(width: usize, height: usize) -> Result<usize, ScanError> {
    let pixels = width.checked_mul(height).ok_or(ScanError::InvalidImage)?;
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || pixels > MAX_PIXELS
    {
        return Err(ScanError::InvalidImage);
    }
    Ok(pixels)
}

/// Accumulate every display before choosing. Repeated copies of the same code
/// are harmless; different credentials must never silently choose a host.
#[derive(Default)]
pub struct Scan {
    pairing: Option<Pairing>,
    codes: usize,
}
impl Scan {
    pub fn add_greyscale(
        &mut self,
        width: usize,
        height: usize,
        pixels: &[u8],
    ) -> Result<(), ScanError> {
        if pixels.len() != pixel_count(width, height)? {
            return Err(ScanError::InvalidImage);
        }
        let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| {
            pixels[y * width + x]
        });
        let grids = image.detect_grids();
        self.codes += grids.len();
        if self.codes > 32 {
            return Err(ScanError::TooManyCodes);
        }
        for grid in grids {
            if let Ok((_, text)) = grid.decode()
                && let Ok(pairing) = Pairing::parse(&text)
            {
                if self
                    .pairing
                    .as_ref()
                    .is_some_and(|previous| previous != &pairing)
                {
                    return Err(ScanError::Ambiguous);
                }
                self.pairing = Some(pairing);
            }
        }
        Ok(())
    }

    pub fn finish(self) -> Result<Pairing, ScanError> {
        self.pairing.ok_or(ScanError::NoPairing)
    }
}
