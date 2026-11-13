//! Golden tests: synthetic ECG files converted by the original EcgPaint.exe on Windows
//! (tests/golden/<case>, see tests/golden/README.md) against ours: file names and formats, the
//! .txt byte for byte, the list of drawing calls against the original's GDI trace (when the case
//! has prims_*.txt), and the pixels outside text rectangles. Diff images go to
//! research/scratch/rust/ (when that folder exists).

use ecg_paint::convert::{self, Options};
use ecg_paint::render::layout::Layout;
use ecg_paint::render::text::Font;
use ecg_paint::render::{compact_log, Page, RectI};
use ecg_paint::settings::{self, Flags, Ini};
use std::path::{Path, PathBuf};

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden");
const SCRATCH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/research/scratch/rust");

struct Case {
    dir: &'static str,
    /// Input file, relative to tests/golden.
    input: &'static str,
    /// Base of the output file names.
    base: &'static str,
}

const CASES: [Case; 9] = [
    Case { dir: "synth_m0", input: "synth_m0/synth_rest_m0.ecg", base: "20040113093015" },
    Case { dir: "synth_m0_thick", input: "synth_m0/synth_rest_m0.ecg", base: "20040113093015" },
    Case { dir: "synth_m0_thick_1280", input: "synth_m0/synth_rest_m0.ecg", base: "20040113093015" },
    Case { dir: "synth_m4", input: "synth_m4/synth_m4.ecg", base: "20040113093015" },
    Case { dir: "synth_m5", input: "synth_m5/synth_m5.ecg", base: "20040113093015" },
    Case { dir: "synth_m16", input: "synth_m16/synth_m16.ecg", base: "20040113093015" },
    Case { dir: "synth_edited_short", input: "synth_edited_short/synth_edited_short.ecg", base: "20040113093015" },
    Case { dir: "synth_exer", input: "synth_exer/synth_exer.ecg", base: "20040113093015" },
    Case { dir: "synth_fields", input: "synth_fields/synth_fields.ecg", base: "PT-2004-000123_20040113140509" },
];

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ecg-paint-golden-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The INI used for the golden run, copied so write-backs never touch the repository.
fn ini_copy(case: &Path, dir: &Path) -> PathBuf {
    let p = dir.join("ecgviewer.ini");
    std::fs::copy(case.join("ecgviewer.ini"), &p).unwrap();
    p
}

fn decode_png(path: &Path) -> (usize, usize, bool, Vec<u8>) {
    let dec = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut r = dec.read_info().unwrap();
    let interlaced = r.info().interlaced;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb, "{}", path.display());
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    buf.truncate(info.buffer_size());
    (info.width as usize, info.height as usize, interlaced, buf)
}

fn in_rects(rects: &[RectI], x: i32, y: i32) -> bool {
    rects.iter().any(|r| x >= r.l && x < r.r && y >= r.t && y < r.b)
}

/// Differing pixels (outside text rects, inside text rects) and a diff image:
/// grey = same, red = only in the original, blue = only ours / different colour.
fn diff(ours: &Page, gold: &[u8]) -> (usize, usize, Vec<u8>, Vec<(i32, i32)>) {
    let (mut outside, mut inside, mut first) = (0, 0, Vec::new());
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
                if first.len() < 20 {
                    first.push((x as i32, y as i32));
                }
                if a == [255, 255, 255] {
                    [255, 0, 0]
                } else {
                    [0, 0, 255]
                }
            };
            img[i..i + 3].copy_from_slice(&px);
        }
    }
    (outside, inside, img, first)
}

fn files_and_txt(c: &Case) {
    let case = Path::new(GOLDEN).join(c.dir);
    let dir = tmp_dir(&format!("files-{}", c.dir));
    let out = dir.join("out");
    let opts = Options {
        input: Path::new(GOLDEN).join(c.input),
        flags: Flags::default(),
        ini: ini_copy(&case, &dir),
        out_dir: Some(out.clone()),
        ecaps: None,
        font: None,
    };
    let res = convert::run(&opts).expect("conversion");
    let mut names: Vec<String> =
        res.files.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    names.sort();
    let base = c.base;
    assert_eq!(names, [format!("{base}.txt"), format!("{base}_6CH.png"), format!("{base}_AVE.png")], "{}", c.dir);

    let txt = std::fs::read(out.join(format!("{base}.txt"))).unwrap();
    let gold = std::fs::read(case.join(format!("{base}.txt"))).unwrap();
    assert_eq!(String::from_utf8_lossy(&txt), String::from_utf8_lossy(&gold), "{}", c.dir);
    assert_eq!(txt, gold, "{}: .txt must be byte-identical", c.dir);

    for page in ["AVE", "6CH"] {
        let (w, h, interlaced, _) = decode_png(&out.join(format!("{base}_{page}.png")));
        let (gw, gh, ginter, _) = decode_png(&case.join(format!("{base}_{page}.png")));
        assert_eq!((w, h, interlaced), (gw, gh, ginter), "{} {page}: size / interlace", c.dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn drawing_calls_and_pixels(c: &Case) {
    let case = Path::new(GOLDEN).join(c.dir);
    let dir = tmp_dir(&format!("pixels-{}", c.dir));
    let ini = Ini::new(ini_copy(&case, &dir));
    let s = settings::load(&ini, &Flags::default());
    let data = std::fs::read(Path::new(GOLDEN).join(c.input)).unwrap();
    let rec = ecg_paint::format::parse(&data, ecg_paint::format::ParseOptions { english: !s.japanese }, None).unwrap();
    let prep = convert::prepare(&rec, &s);
    let lo = Layout::for_resolution(s.resolution);
    let font = Font::new(lo.font_h, lo.font_w, None).unwrap();
    let (ave, six) = convert::render_pages(&rec, &s, &prep, &ini, &font, settings::rhythm_lead(&ini));

    let mut report = Vec::new();
    let mut ok = true;
    for (name, page) in [("AVE", &ave), ("6CH", &six)] {
        // 1. drawing calls, in order, against the original's GDI trace (cases traced under Wine)
        if let Ok(want) = std::fs::read_to_string(case.join(format!("prims_{name}.txt"))) {
            let want: Vec<&str> = want.lines().collect();
            let got = compact_log(&page.log);
            let first_bad =
                (0..want.len().max(got.len())).find(|&i| want.get(i).copied() != got.get(i).map(|s| s.as_str()));
            if let Some(i) = first_bad {
                ok = false;
                report.push(format!(
                    "{} {name}: drawing call {i} differs: original {:?}, ours {:?}",
                    c.dir,
                    want.get(i),
                    got.get(i)
                ));
            }
        }
        // 2. pixels
        let (gw, gh, _, gold) = decode_png(&case.join(format!("{}_{name}.png", c.base)));
        assert_eq!((gw, gh), (page.w, page.h));
        let (outside, inside, img, first) = diff(page, &gold);
        report.push(format!(
            "{} {name}: {outside} differing pixels outside text rects, {inside} inside text rects; first: {first:?}",
            c.dir
        ));
        if outside > 0 {
            ok = false;
        }
        if Path::new(SCRATCH).is_dir() {
            let png = ecg_paint::encode::png(&img, page.w, page.h, 6);
            let _ = std::fs::write(Path::new(SCRATCH).join(format!("diff_{}_{name}.png", c.dir)), png);
            let ours = ecg_paint::encode::png(&page.rgb, page.w, page.h, 6);
            let _ = std::fs::write(Path::new(SCRATCH).join(format!("ours_{}_{name}.png", c.dir)), ours);
        }
    }
    for l in &report {
        eprintln!("{l}");
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok, "{}", report.join("\n"));
}

#[test]
fn golden_files_and_txt() {
    for c in &CASES {
        files_and_txt(c);
    }
}

#[test]
fn golden_drawing_calls_and_pixels() {
    for c in &CASES {
        drawing_calls_and_pixels(c);
    }
}
