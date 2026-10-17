//! A name's avatar as plain pixels, for apps that may not load images from
//! URLs: Basecamp's plugin sandbox refuses `data:` URLs, so an app can't hand
//! the `avatar` entry to an image element. The module decodes it instead and
//! answers `{width, height, pixels}`, `pixels` being base64 of RGBA8 bytes, row
//! by row from the top left, for the app to paint itself.
//!
//! The entry is written by the name's holder, so everything here is bounded:
//! the text is capped before it is decoded, the image's declared size is
//! checked before any pixel is, and what comes back is at most
//! [`MAX_SIDE`] pixels a side (larger images are scaled down, nearest
//! neighbour, so pixel art stays crisp). Only PNG is decoded; another format
//! (JPEG, GIF, WebP) gives no pixels, and the app shows its own seal.

use serde_json::{json, Value};

/// The most pixels a side the answer has.
pub const MAX_SIDE: u32 = 128;
/// The longest base64 payload decoded (a profile holds 12 KiB, so this is
/// far more than a real entry needs).
const MAX_BASE64: usize = 256 * 1024;
/// The largest image decoded, a side, before scaling down.
const MAX_SOURCE_SIDE: u32 = 1024;

/// The pixels of an `avatar` entry (`data:image/png;base64,...`), or None if
/// it is not an image this module can decode within its limits.
pub fn pixels(entry: &str) -> Option<Value> {
    let (width, height, rgba) = decode(entry)?;
    Some(json!({ "width": width, "height": height, "pixels": base64_encode(&rgba) }))
}

/// The entry decoded and scaled down: `(width, height, RGBA8 bytes)`.
fn decode(entry: &str) -> Option<(u32, u32, Vec<u8>)> {
    let payload = entry.trim().strip_prefix("data:image/png;base64,")?;
    if payload.len() > MAX_BASE64 {
        return None;
    }
    let bytes = base64_decode(payload)?;
    let (width, height, rgba) = decode_png(&bytes)?;
    Some(fit(width, height, &rgba))
}

/// A PNG as RGBA8, if its sides are within [`MAX_SOURCE_SIDE`].
fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let limits = png::Limits { bytes: (MAX_SOURCE_SIDE * MAX_SOURCE_SIDE * 8) as usize };
    let mut decoder = png::Decoder::new_with_limits(std::io::Cursor::new(bytes), limits);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0 || height == 0 || width > MAX_SOURCE_SIDE || height > MAX_SOURCE_SIDE {
        return None;
    }
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    let data = &buffer[..frame.buffer_size()];
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        // EXPAND turns a palette into RGB or RGBA.
        png::ColorType::Indexed => return None,
    };
    if frame.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for row in data.chunks(frame.line_size).take(height as usize) {
        for pixel in row.chunks_exact(channels).take(width as usize) {
            rgba.extend_from_slice(&match *pixel {
                [v] => [v, v, v, 255],
                [v, a] => [v, v, v, a],
                [r, g, b] => [r, g, b, 255],
                [r, g, b, a] => [r, g, b, a],
                _ => return None,
            });
        }
    }
    (rgba.len() == (width * height * 4) as usize).then_some((width, height, rgba))
}

/// The image scaled down, nearest neighbour, to at most [`MAX_SIDE`] a side,
/// keeping its shape. A small enough image comes back as it is.
fn fit(width: u32, height: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
    let longest = width.max(height);
    if longest <= MAX_SIDE {
        return (width, height, rgba.to_vec());
    }
    let scaled = |side: u32| (u64::from(side) * u64::from(MAX_SIDE) / u64::from(longest)).max(1) as u32;
    let (out_width, out_height) = (scaled(width), scaled(height));
    let mut out = Vec::with_capacity((out_width * out_height * 4) as usize);
    for y in 0..out_height {
        let source_y = (u64::from(y) * u64::from(height) / u64::from(out_height)) as usize;
        for x in 0..out_width {
            let source_x = (u64::from(x) * u64::from(width) / u64::from(out_width)) as usize;
            let at = (source_y * width as usize + source_x) * 4;
            out.extend_from_slice(&rgba[at..at + 4]);
        }
    }
    (out_width, out_height, out)
}

const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            out.push(if i <= chunk.len() { TABLE[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// Standard base64 with optional `=` padding, or None if it is not that.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let body = text.trim_end_matches('=');
    if text.len() - body.len() > 2 || body.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    let (mut bits, mut count) = (0u32, 0u32);
    for byte in body.bytes() {
        let value = TABLE.iter().position(|&c| c == byte)? as u32;
        bits = bits << 6 | value;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// jef.lez's avatar as it is on the chain: 24×24 pixel art, a palette PNG.
    pub const JEF: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABgAAAAYCAMAAADXqc3KAAAAOVBMVEURERAIBw1NdEI6WDKW1ngSEBoAAAB9omkjNR54eHdeclNzc3b/AACBin5XcUkoGwnGxsbiWyZCUDorO/AgAAAAmElEQVQoz7WRwRLCIAxEWSBJrUrV//9YE2g1UDy6w+Swb5YwSwj/EIC5HWOcILNN+OGPBCnlppQw+NTUE6jPInYoLx5kkkOUcfEBkXUVGxq59uCmN+k4AwvocMB8t4PyHQ4UM5knoLRAoe+Dd1DFVSPYNn6YnrwDslcpeB2J4IG0u3QDfToB1Uq4lrjg1JXZXYX2rRUA7tvf2UsHKzvMA6UAAAAASUVORK5CYII=";

    fn png(width: u32, height: u32, color: png::ColorType, data: &[u8]) -> String {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(data).unwrap();
        format!("data:image/png;base64,{}", base64_encode(&bytes))
    }

    #[test]
    fn jef_decodes_to_his_24_by_24_pixels() {
        let (width, height, rgba) = decode(JEF).expect("jef's avatar decodes");
        assert_eq!((width, height, rgba.len()), (24, 24, 24 * 24 * 4));
        // Opaque pixel art in a handful of palette colours.
        let colours: std::collections::HashSet<&[u8]> = rgba.chunks(4).collect();
        assert!(rgba.chunks(4).all(|p| p[3] == 255) && (4..=32).contains(&colours.len()));
        let value = pixels(JEF).unwrap();
        assert_eq!((value["width"].as_u64(), value["height"].as_u64()), (Some(24), Some(24)));
        assert_eq!(base64_decode(value["pixels"].as_str().unwrap()).unwrap(), rgba);
        if std::env::var_os("PRINT_JEF").is_some() {
            println!("JEF_RGBA={}", value["pixels"].as_str().unwrap());
        }
    }

    #[test]
    fn base64_round_trips() {
        for length in 0..10 {
            let bytes: Vec<u8> = (0..length).map(|i| (i * 37 + 5) as u8).collect();
            assert_eq!(base64_decode(&base64_encode(&bytes)).unwrap(), bytes);
        }
        assert_eq!(base64_decode("not base64!"), None);
        assert_eq!(base64_decode("QQ==="), None);
    }

    #[test]
    fn colour_types_become_rgba() {
        let grey = png(2, 1, png::ColorType::Grayscale, &[0, 200]);
        assert_eq!(decode(&grey).unwrap().2, vec![0, 0, 0, 255, 200, 200, 200, 255]);
        let rgb = png(1, 1, png::ColorType::Rgb, &[1, 2, 3]);
        assert_eq!(decode(&rgb).unwrap().2, vec![1, 2, 3, 255]);
        let grey_alpha = png(1, 1, png::ColorType::GrayscaleAlpha, &[9, 7]);
        assert_eq!(decode(&grey_alpha).unwrap().2, vec![9, 9, 9, 7]);
    }

    #[test]
    fn large_images_are_scaled_down_nearest_neighbour() {
        // 256×64, left half red, right half blue.
        let data: Vec<u8> = (0..64 * 256)
            .flat_map(|i| if i % 256 < 128 { [255, 0, 0, 255] } else { [0, 0, 255, 255] })
            .collect();
        let (width, height, rgba) = decode(&png(256, 64, png::ColorType::Rgba, &data)).unwrap();
        assert_eq!((width, height), (128, 32));
        assert_eq!(&rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba[rgba.len() - 4..], &[0, 0, 255, 255]);
    }

    #[test]
    fn hostile_or_unsupported_entries_give_nothing() {
        // Declares 2000×2000: refused from its header, before any pixel.
        let data = vec![0u8; 2000 * 2000];
        assert_eq!(pixels(&png(2000, 2000, png::ColorType::Grayscale, &data)), None);
        // Too long to decode at all.
        let long = format!("data:image/png;base64,{}", "A".repeat(MAX_BASE64 + 4));
        assert_eq!(pixels(&long), None);
        for entry in [
            "",
            "https://example.com/a.png",
            "data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=",
            "data:image/jpeg;base64,/9j/4AAQ",
            "data:image/png;base64,iVBORw0KGgo=",
            "data:image/png;base64,@@@@",
        ] {
            assert_eq!(pixels(entry), None, "{entry}");
        }
    }
}
