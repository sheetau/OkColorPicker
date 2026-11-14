use okcolor_csp::{
    Pairing,
    qr::{Scan, ScanError, pixel_count},
};
use qrcode::{Color, QrCode};

const URL: &str = "https://companion.clip-studio.com/rc/en-us?s=458ba0757b17e25a83bc6e7a28ef4081a3694352bf17c0f72f3a56d33391a3617811e842";

fn fixture(text: &str, scale: usize, rotation: usize) -> (usize, Vec<u8>) {
    let code = QrCode::new(text).unwrap();
    let size = (code.width() + 8) * scale;
    let mut pixels = vec![240; size * size];
    for y in 0..code.width() {
        for x in 0..code.width() {
            for dy in 0..scale {
                for dx in 0..scale {
                    let mut px = (x + 4) * scale + dx;
                    let mut py = (y + 4) * scale + dy;
                    for _ in 0..rotation {
                        (px, py) = (size - 1 - py, px);
                    }
                    pixels[py * size + px] = if code[(x, y)] == Color::Dark { 20 } else { 240 };
                }
            }
        }
    }
    (size, pixels)
}

#[test]
fn reads_scaled_rotated_pairing_without_network_or_image_files() {
    for scale in [2, 4, 7] {
        for rotation in 0..4 {
            let (size, pixels) = fixture(URL, scale, rotation);
            let mut scan = Scan::default();
            scan.add_greyscale(size, size, &pixels).unwrap();
            assert_eq!(scan.finish().unwrap(), Pairing::parse(URL).unwrap());
        }
    }
}

#[test]
fn ignores_other_qr_and_rejects_ambiguous_credentials() {
    let mut scan = Scan::default();
    let (size, pixels) = fixture("https://example.com/", 4, 0);
    scan.add_greyscale(size, size, &pixels).unwrap();
    let (size, pixels) = fixture(URL, 4, 0);
    scan.add_greyscale(size, size, &pixels).unwrap();
    scan.add_greyscale(size, size, &pixels).unwrap();
    assert_eq!(scan.finish().unwrap(), Pairing::parse(URL).unwrap());
    let mut scan = Scan::default();
    scan.add_greyscale(size, size, &pixels).unwrap();
    // Change the synthetic generation, keeping the URL well formed.
    let (size, different) = fixture(&format!("{}43", &URL[..URL.len() - 2]), 4, 0);
    assert_eq!(
        scan.add_greyscale(size, size, &different),
        Err(ScanError::Ambiguous)
    );
}

#[test]
fn blank_malformed_and_oversized_images_are_bounded() {
    let mut scan = Scan::default();
    scan.add_greyscale(32, 32, &[255; 1024]).unwrap();
    assert_eq!(scan.finish().unwrap_err(), ScanError::NoPairing);
    assert_eq!(
        Scan::default().add_greyscale(8, 8, &[0; 63]),
        Err(ScanError::InvalidImage)
    );
    for (w, h) in [(0, 1), (usize::MAX, 2), (8193, 1), (8192, 8192)] {
        assert_eq!(pixel_count(w, h), Err(ScanError::InvalidImage));
    }
}
