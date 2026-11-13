//! Pixel comparison of our pages with the original's images, text rectangles excluded (the same
//! check as tests/golden.rs, for any folder made by research/recheck/recheck_variants.ps1):
//!
//! `golden_diff VARIANT_DIR...`  — each folder holds in.ecg, rust.ini (the INI as run),
//! bin/t_ecaps12c.dll and orig/<base>_AVE.png, orig/<base>_6CH.png.
//! Prints, per page, the differing pixels outside and inside the text rectangles; exit 1 when
//! any page differs outside text. Diff images go to <dir>/diff_<page>.png.

use ecg_paint::analtext::AnalTables;
use ecg_paint::convert;
use ecg_paint::render::text::Font;
use ecg_paint::render::{Page, RectI};
use ecg_paint::settings::{self, Flags, Ini};
use std::path::Path;

fn decode_png(path: &Path) -> Option<(usize, usize, Vec<u8>)> {
    let dec = png::Decoder::new(std::fs::File::open(path).ok()?);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgb || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    Some((info.width as usize, info.height as usize, buf))
}

fn in_rects(rects: &[RectI], x: i32, y: i32) -> bool {
    rects.iter().any(|r| x >= r.l && x < r.r && y >= r.t && y < r.b)
}

fn diff(ours: &Page, gold: &[u8]) -> (usize, usize, Vec<u8>) {
    let (mut outside, mut inside) = (0, 0);
    let mut img = vec![0u8; ours.w * ours.h * 3];
    for y in 0..ours.h {
        for x in 0..ours.w {
            let i = (y * ours.w + x) * 3;
            let (a, b) = (&ours.rgb[i..i + 3], &gold[i..i + 3]);
            let px: [u8; 3] = if a == b {
                let g = if a == [255, 255, 255] { 255 } else { 200 };
                [g, g, g]
            } else if in_rects(&ours.text_rects, x as i32, y as i32) {
                inside += 1;
                [255, 255, 160]
            } else {
                outside += 1;
                if a == [255, 255, 255] { [255, 0, 0] } else { [0, 0, 255] }
            };
            img[i..i + 3].copy_from_slice(&px);
        }
    }
    (outside, inside, img)
}

fn main() {
    let mut bad = 0;
    for arg in std::env::args().skip(1) {
        let dir = Path::new(&arg);
        let name = dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let ini = Ini::new(dir.join("rust.ini"));
        let flags = match std::fs::read_to_string(dir.join("flags.txt")) {
            Ok(f) if f.trim().to_uppercase().starts_with("-H") => {
                Flags { h_option: Some(settings::atoi(&f.trim()[2..])), ..Flags::default() }
            }
            _ => Flags::default(),
        };
        let s = settings::load(&ini, &flags);
        let data = std::fs::read(dir.join("in.ecg")).expect("in.ecg");
        let tables = std::fs::read(dir.join("bin").join("t_ecaps12c.dll")).ok().and_then(|b| AnalTables::from_dll(&b).ok());
        let rec = match ecg_paint::format::parse(&data, ecg_paint::format::ParseOptions { english: !s.japanese }, tables.as_ref()) {
            Ok(r) => r,
            Err(e) => {
                println!("{name:14} parse error: {e}");
                bad += 1;
                continue;
            }
        };
        let prep = convert::prepare(&rec, &s);
        let lo = ecg_paint::render::layout::Layout::for_resolution(s.resolution);
        let font = Font::new(lo.font_h, lo.font_w, None).unwrap();
        let (ave, six) = convert::render_pages(&rec, &s, &prep, &ini, &font, settings::rhythm_lead(&ini));
        for (page_name, page) in [("AVE", &ave), ("6CH", &six)] {
            let gold = std::fs::read_dir(dir.join("orig"))
                .ok()
                .and_then(|rd| rd.flatten().map(|e| e.path()).find(|p| p.to_string_lossy().ends_with(&format!("_{page_name}.png"))));
            let Some(gold) = gold else {
                println!("{name:14} {page_name}: no original image (the original did not write one)");
                continue;
            };
            let Some((w, h, g)) = decode_png(&gold) else {
                println!("{name:14} {page_name}: cannot decode {}", gold.display());
                bad += 1;
                continue;
            };
            if (w, h) != (page.w, page.h) {
                println!("{name:14} {page_name}: size {}x{} vs ours {}x{}", w, h, page.w, page.h);
                bad += 1;
                continue;
            }
            let (outside, inside, img) = diff(page, &g);
            println!("{name:14} {page_name}: {outside:6} differing pixels outside text rects, {inside:6} inside");
            if outside > 0 {
                bad += 1;
                let _ = std::fs::write(dir.join(format!("diff_{page_name}.png")), ecg_paint::encode::png(&img, page.w, page.h, 6));
            }
        }
    }
    if bad > 0 {
        std::process::exit(1);
    }
}
