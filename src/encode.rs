//! Pixels → file, as imgctl.dll does it for EcgPaint: PNG (8-bit RGB, Adam7 interlaced,
//! adaptive filters, IDAT chunks of 8192 bytes, zlib level PNG_COMPLEVEL), JPEG (quality
//! JPEG_QUALITY) or a 24-bit BMP.

use crate::settings::ImageFormat;

pub fn encode(rgb: &[u8], w: usize, h: usize, fmt: ImageFormat, png_level: i32, jpeg_quality: i32) -> Result<Vec<u8>, String> {
    match fmt {
        ImageFormat::Png => Ok(png(rgb, w, h, png_level)),
        ImageFormat::Bmp => Ok(bmp(rgb, w, h)),
        ImageFormat::Jpeg => jpeg(rgb, w, h, jpeg_quality),
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend(&body);
    out.extend(crc32(&body).to_be_bytes());
}

fn paeth(a: i32, b: i32, c: i32) -> i32 {
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Filter one scanline with the filter of minimum sum of |signed residuals| (libpng heuristic).
fn filter_row(cur: &[u8], prev: &[u8], out: &mut Vec<u8>) {
    const BPP: usize = 3;
    let mut best: Option<(u64, u8, Vec<u8>)> = None;
    for f in 0..5u8 {
        let mut row = Vec::with_capacity(cur.len());
        for i in 0..cur.len() {
            let a = if i >= BPP { cur[i - BPP] as i32 } else { 0 };
            let b = prev[i] as i32;
            let c = if i >= BPP { prev[i - BPP] as i32 } else { 0 };
            let pred = match f {
                0 => 0,
                1 => a,
                2 => b,
                3 => (a + b) / 2,
                _ => paeth(a, b, c),
            };
            row.push((cur[i] as i32 - pred) as u8);
        }
        let sum: u64 = row.iter().map(|&v| (v as i8).unsigned_abs() as u64).sum();
        if best.as_ref().map_or(true, |b| sum < b.0) {
            best = Some((sum, f, row));
        }
    }
    let (_, f, row) = best.unwrap_or((0, 0, cur.to_vec()));
    out.push(f);
    out.extend(row);
}

pub fn png(rgb: &[u8], w: usize, h: usize, level: i32) -> Vec<u8> {
    // Adam7 passes: (x0, y0, dx, dy)
    const PASSES: [(usize, usize, usize, usize); 7] =
        [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)];
    let mut raw = Vec::new();
    for (x0, y0, dx, dy) in PASSES {
        if x0 >= w || y0 >= h {
            continue;
        }
        let pw = (w - x0).div_ceil(dx);
        let mut prev = vec![0u8; pw * 3];
        let mut y = y0;
        while y < h {
            let mut cur = Vec::with_capacity(pw * 3);
            let mut x = x0;
            while x < w {
                let i = (y * w + x) * 3;
                cur.extend_from_slice(&rgb[i..i + 3]);
                x += dx;
            }
            filter_row(&cur, &prev, &mut raw);
            prev = cur;
            y += dy;
        }
    }
    let level = level.clamp(0, 9) as u8;
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, level);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend((w as u32).to_be_bytes());
    ihdr.extend((h as u32).to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 1]);
    chunk(&mut out, b"IHDR", &ihdr);
    for part in z.chunks(8192) {
        chunk(&mut out, b"IDAT", part);
    }
    chunk(&mut out, b"IEND", &[]);
    out
}

pub fn bmp(rgb: &[u8], w: usize, h: usize) -> Vec<u8> {
    let stride = (w * 3 + 3) & !3;
    let size = 54 + stride * h;
    let mut out = Vec::with_capacity(size);
    out.extend(b"BM");
    out.extend((size as u32).to_le_bytes());
    out.extend([0u8; 4]);
    out.extend(54u32.to_le_bytes());
    out.extend(40u32.to_le_bytes());
    out.extend((w as i32).to_le_bytes());
    out.extend((h as i32).to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(24u16.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend(((stride * h) as u32).to_le_bytes());
    out.extend([0u8; 16]);
    for y in (0..h).rev() {
        let row = &rgb[y * w * 3..(y + 1) * w * 3];
        for p in row.chunks(3) {
            out.extend([p[2], p[1], p[0]]);
        }
        out.extend(std::iter::repeat(0u8).take(stride - w * 3));
    }
    out
}

fn jpeg(rgb: &[u8], w: usize, h: usize, quality: i32) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let q = quality.clamp(1, 100) as u8;
    let enc = jpeg_encoder::Encoder::new(&mut out, q);
    enc.encode(rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trip() {
        let (w, h) = (13usize, 7usize);
        let rgb: Vec<u8> = (0..w * h * 3).map(|i| (i * 37 % 251) as u8).collect();
        let data = png(&rgb, w, h, 6);
        let dec = png::Decoder::new(&data[..]);
        let mut reader = dec.read_info().unwrap();
        assert!(reader.info().interlaced);
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (13, 7));
        assert_eq!(&buf[..w * h * 3], &rgb[..]);
    }

    #[test]
    fn bmp_header() {
        let b = bmp(&[1, 2, 3, 4, 5, 6], 2, 1);
        assert_eq!(&b[0..2], b"BM");
        assert_eq!(b.len(), 54 + 8);
        assert_eq!(&b[54..60], &[3, 2, 1, 6, 5, 4]);
    }

    #[test]
    fn crc() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }
}
