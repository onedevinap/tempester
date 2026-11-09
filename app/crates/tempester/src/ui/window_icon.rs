//! The window icon (title bar and task bar on Windows and Linux; macOS takes the Dock icon from the app bundle).
//! The pixels are the 64 px export of the app icon, decoded to raw RGBA by `tools/icons/png_to_rgba.py`.

use tao::window::Icon;

const SIZE: u32 = 64;
const RGBA: &[u8] = include_bytes!("../../icons/tempest_64.rgba");

/// The icon, or `None` if the embedded pixels are unusable (the window then keeps the system icon).
pub fn icon() -> Option<Icon> {
    from_rgba(RGBA, SIZE)
}

/// The size is checked here: tao refuses a wrong length on macOS and Windows but not on Linux, where a short buffer
/// would be read past its end by GTK.
fn from_rgba(rgba: &[u8], size: u32) -> Option<Icon> {
    let expected = usize::try_from(size)
        .ok()?
        .checked_mul(usize::try_from(size).ok()?)?
        .checked_mul(4)?;
    if rgba.len() != expected {
        return None;
    }
    Icon::from_rgba(rgba.to_vec(), size, size).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(x: usize, y: usize) -> &'static [u8] {
        let at = (y * SIZE as usize + x) * 4;
        &RGBA[at..at + 4]
    }

    #[test]
    fn test_icon_when_embedded_pixels_returns_an_icon() {
        assert_eq!(RGBA.len(), (SIZE * SIZE * 4) as usize);
        assert!(icon().is_some());
    }

    #[test]
    fn test_icon_when_drawing_returns_transparent_corner_and_opaque_panel() {
        assert_eq!(pixel(0, 0)[3], 0);
        assert_eq!(pixel(32, 32)[3], 255);
    }

    #[test]
    fn test_from_rgba_when_size_does_not_match_returns_none() {
        assert!(from_rgba(&RGBA[..RGBA.len() - 4], SIZE).is_none());
    }
}
