//! Pictures on the clipboard: reading the formats Windows apps copy,
//! writing them back, and telling copies of the same picture apart.

use crate::icon::encode_png;

/// Bigger pictures (like a 7000×6000 photo) aren't kept in the history.
const MAX_PIXELS: u64 = 40_000_000;

/// A copied picture, ready to keep in the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipImage {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// The same picture always gets the same hash, however it was copied.
    pub hash: String,
}

/// From a Windows bitmap (the `CF_DIB` clipboard format): a header followed
/// by rows of pixels. Handles the 24- and 32-bit pictures that screenshots
/// and apps copy.
pub fn from_dib(dib: &[u8]) -> Result<ClipImage, String> {
    let bytes = |at: usize, len: usize| dib.get(at..at + len).ok_or("The picture is incomplete.");
    let u32_at = |at| bytes(at, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let u16_at = |at| bytes(at, 2).map(|b| u16::from_le_bytes([b[0], b[1]]));

    let header_size = u32_at(0)? as usize;
    let width = u32_at(4)? as i32;
    let height = u32_at(8)? as i32;
    let bits = u16_at(14)?;
    let compression = u32_at(16)?;
    let palette_entries = u32_at(32)? as usize;

    const RGB: u32 = 0;
    const BITFIELDS: u32 = 3;
    if !matches!(bits, 24 | 32) || !matches!(compression, RGB | BITFIELDS) {
        return Err("This kind of picture isn't supported.".into());
    }
    if width <= 0 || height == 0 {
        return Err("The picture is empty.".into());
    }
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return Err("The picture is too big to keep.".into());
    }

    // The basic header is followed by three color masks when bitfields
    // are used; bigger headers include them.
    let masks = if compression == BITFIELDS && header_size == 40 {
        12
    } else {
        0
    };
    let start = header_size + masks + palette_entries * 4;
    let stride = (w * usize::from(bits)).div_ceil(32) * 4;
    let pixels = bytes(start, stride * h)?;
    let per_pixel = usize::from(bits / 8);

    let mut rgba = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        // Positive heights store the bottom row first.
        let source_row = if height > 0 { h - 1 - row } else { row };
        let line = &pixels[source_row * stride..][..w * per_pixel];
        for px in line.chunks_exact(per_pixel) {
            let alpha = if bits == 32 { px[3] } else { 255 };
            rgba.extend_from_slice(&[px[2], px[1], px[0], alpha]);
        }
    }
    // Most apps leave the alpha of 32-bit pictures at zero, meaning "no
    // transparency" rather than "invisible".
    let (pixels, _) = rgba.as_chunks_mut::<4>();
    if bits == 32 && pixels.iter().all(|p| p[3] == 0) {
        pixels.iter_mut().for_each(|p| p[3] = 255);
    }
    let png = encode_png(w as u32, h as u32, &rgba)?;
    Ok(finish(png, w as u32, h as u32, &rgba))
}

/// From PNG bytes (the "PNG" clipboard format browsers and Office use).
pub fn from_png(png: &[u8]) -> Result<ClipImage, String> {
    let (width, height, rgba) = decode_rgba(png)?;
    Ok(finish(png.to_vec(), width, height, &rgba))
}

fn finish(png: Vec<u8>, width: u32, height: u32, rgba: &[u8]) -> ClipImage {
    ClipImage {
        png,
        width,
        height,
        hash: fingerprint(width, height, rgba),
    }
}

/// A Windows bitmap for putting a picture back on the clipboard: 32-bit,
/// bottom row first, the most widely understood layout. Transparent parts
/// are shown on white, since many apps ignore transparency here (the PNG
/// copied alongside keeps it for the apps that don't).
pub fn to_dib(png: &[u8]) -> Result<Vec<u8>, String> {
    let (width, height, rgba) = decode_rgba(png)?;
    let row_bytes = width as usize * 4;
    let mut dib = Vec::with_capacity(40 + row_bytes * height as usize);
    for value in [40, width, height] {
        dib.extend_from_slice(&value.to_le_bytes());
    }
    dib.extend_from_slice(&1u16.to_le_bytes()); // planes
    dib.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    dib.extend_from_slice(&0u32.to_le_bytes()); // uncompressed
    dib.extend_from_slice(&((row_bytes * height as usize) as u32).to_le_bytes());
    dib.extend_from_slice(&[0; 16]); // resolution and palette: unused

    let over_white = |channel: u8, alpha: u8| {
        let (c, a) = (u32::from(channel), u32::from(alpha));
        ((c * a + 255 * (255 - a) + 127) / 255) as u8
    };
    for row in rgba.chunks_exact(row_bytes).rev() {
        for &[r, g, b, a] in row.as_chunks::<4>().0 {
            dib.extend_from_slice(&[over_white(b, a), over_white(g, a), over_white(r, a), 255]);
        }
    }
    Ok(dib)
}

fn decode_rgba(png: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(png);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let (width, height) = (reader.info().width, reader.info().height);
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("The picture is too big to keep.".into());
    }
    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    buffer.truncate(frame.buffer_size());

    let rgba = match frame.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buffer
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[gray, alpha]| [gray, gray, gray, alpha])
            .collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("Unexpected palette picture.".into()),
    };
    Ok((width, height, rgba))
}

/// FNV-1a over the size and pixels: stable across versions, unlike Rust's
/// built-in hasher, since hashes are saved.
fn fingerprint(width: u32, height: u32, rgba: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let size = [width.to_le_bytes(), height.to_le_bytes()].concat();
    for &byte in size.iter().chain(rgba) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2×2 24-bit bitmap, bottom row first, with rows padded to 4 bytes:
    /// top row red, blue; bottom row green, white.
    fn dib_24() -> Vec<u8> {
        let mut dib = Vec::new();
        for value in [40u32, 2, 2] {
            dib.extend_from_slice(&value.to_le_bytes());
        }
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&24u16.to_le_bytes());
        dib.extend_from_slice(&[0; 24]);
        dib.extend_from_slice(&[0, 255, 0, 255, 255, 255, 0, 0]); // green, white + padding
        dib.extend_from_slice(&[0, 0, 255, 255, 0, 0, 0, 0]); // red, blue + padding
        dib
    }

    fn pixels(image: &ClipImage) -> Vec<u8> {
        decode_rgba(&image.png).unwrap().2
    }

    #[test]
    fn reads_24_bit_bitmaps() {
        let image = from_dib(&dib_24()).unwrap();
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(
            pixels(&image),
            [255, 0, 0, 255, 0, 0, 255, 255, 0, 255, 0, 255, 255, 255, 255, 255]
        );
    }

    #[test]
    fn reads_top_down_32_bit_bitmaps_without_alpha_as_opaque() {
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&1i32.to_le_bytes());
        dib.extend_from_slice(&(-2i32).to_le_bytes()); // top row first
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&[0; 24]);
        dib.extend_from_slice(&[10, 20, 30, 0, 40, 50, 60, 0]);
        let image = from_dib(&dib).unwrap();
        assert_eq!(pixels(&image), [30, 20, 10, 255, 60, 50, 40, 255]);
    }

    #[test]
    fn rejects_broken_and_unsupported_bitmaps() {
        assert!(from_dib(&[]).is_err());
        let mut truncated = dib_24();
        truncated.truncate(50);
        assert!(from_dib(&truncated).is_err());
        let mut eight_bit = dib_24();
        eight_bit[14] = 8;
        assert!(from_dib(&eight_bit).is_err());
    }

    #[test]
    fn the_same_picture_gets_the_same_hash_either_way() {
        let from_bitmap = from_dib(&dib_24()).unwrap();
        let from_png_bytes = from_png(&from_bitmap.png).unwrap();
        assert_eq!(from_bitmap.hash, from_png_bytes.hash);

        let mut other = dib_24();
        let last = other.len() - 5;
        other[last] = 1;
        assert_ne!(from_dib(&other).unwrap().hash, from_bitmap.hash);
    }

    #[test]
    fn round_trips_through_a_bitmap() {
        let image = from_dib(&dib_24()).unwrap();
        let back = from_dib(&to_dib(&image.png).unwrap()).unwrap();
        assert_eq!(back.hash, image.hash);
    }

    #[test]
    fn bitmaps_show_transparency_on_white() {
        let png = encode_png(1, 1, &[0, 0, 0, 0]).unwrap();
        let dib = to_dib(&png).unwrap();
        assert_eq!(&dib[40..], &[255, 255, 255, 255]);
    }
}
