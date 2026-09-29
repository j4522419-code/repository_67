//! Turning raw icon pixels from Windows into PNG files the UI can show.

/// Encodes a top-down 32-bit BGRA bitmap, as Windows hands out, as a PNG.
///
/// Windows icon bitmaps come in two flavors, and this handles both:
/// - with premultiplied alpha, which is converted to the straight alpha PNG
///   expects;
/// - old-style icons with no alpha at all (every alpha byte 0), which are
///   made fully opaque instead of invisible.
pub fn bgra_to_png(width: u32, height: u32, mut pixels: Vec<u8>) -> Result<Vec<u8>, String> {
    let expected = width as usize * height as usize * 4;
    if pixels.len() != expected {
        return Err(format!(
            "expected {expected} bytes of pixels, got {}",
            pixels.len()
        ));
    }

    let (bgra, _) = pixels.as_chunks::<4>();
    let no_alpha = bgra.iter().all(|p| p[3] == 0);
    // In premultiplied data no color channel can exceed alpha.
    let premultiplied = !no_alpha && bgra.iter().all(|p| p[0].max(p[1]).max(p[2]) <= p[3]);

    for p in pixels.as_chunks_mut::<4>().0 {
        p.swap(0, 2); // BGRA -> RGBA
        if no_alpha {
            p[3] = 255;
        } else if premultiplied && p[3] > 0 && p[3] < 255 {
            let alpha = u16::from(p[3]);
            for channel in &mut p[..3] {
                *channel = ((u16::from(*channel) * 255 + alpha / 2) / alpha) as u8;
            }
        }
    }

    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&pixels))
        .map_err(|e| e.to_string())?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
        let decoder = png::Decoder::new(png);
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        buf.truncate(info.buffer_size());
        (info.width, info.height, buf)
    }

    #[test]
    fn converts_bgra_to_rgba() {
        // One opaque pixel: blue=10, green=20, red=30.
        let png = bgra_to_png(1, 1, vec![10, 20, 30, 255]).unwrap();
        assert_eq!(decode(&png), (1, 1, vec![30, 20, 10, 255]));
    }

    #[test]
    fn unpremultiplies_alpha() {
        // Half-transparent white, premultiplied: channels are 128, not 255.
        // The opaque pixel keeps the data consistent with premultiplication.
        let png = bgra_to_png(2, 1, vec![128, 128, 128, 128, 0, 0, 0, 255]).unwrap();
        assert_eq!(decode(&png).2, vec![255, 255, 255, 128, 0, 0, 0, 255]);
    }

    #[test]
    fn leaves_straight_alpha_alone() {
        // A color brighter than its alpha can't be premultiplied.
        let png = bgra_to_png(1, 1, vec![200, 200, 200, 100]).unwrap();
        assert_eq!(decode(&png).2, vec![200, 200, 200, 100]);
    }

    #[test]
    fn makes_alphaless_icons_opaque() {
        let png = bgra_to_png(2, 1, vec![1, 2, 3, 0, 4, 5, 6, 0]).unwrap();
        assert_eq!(decode(&png).2, vec![3, 2, 1, 255, 6, 5, 4, 255]);
    }

    #[test]
    fn rejects_wrong_buffer_size() {
        assert!(bgra_to_png(2, 2, vec![0; 4]).is_err());
    }
}
