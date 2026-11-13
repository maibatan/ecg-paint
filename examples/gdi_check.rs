//! Whole-image comparison with the original, text included (Windows only).
//!
//! The bundled font is not the original's (Arial / MS Gothic), so tests/golden.rs and
//! examples/golden_diff.rs leave the text rectangles out. Here every DrawText call of ours is
//! drawn by Windows GDI itself, with the font, colours and bitmap type the original uses
//! (CreateFontA(h, w, 0, 0, 500, …, OUT_TT_PRECIS, …, 0x21, "Arial"), TRANSPARENT, black, a
//! bitmap compatible with the display), at the point of the page where we draw it. The page
//! must then equal the original's image pixel for pixel: that proves the strings, rectangles,
//! formats and drawing order of the text, which the other checks cannot see.
//!
//! `gdi_check DIR...` — each folder as made by research/recheck/recheck_variants.ps1: in.ecg,
//! rust.ini, bin/t_ecaps12c.dll, optional flags.txt, orig/<base>_AVE.{png,bmp}, _6CH.
//! Exit 1 when any page differs. Diff images go to <dir>/gdidiff_<page>.png, our page to
//! <dir>/gdi_<page>.png.
//!
//! `--qmark` (before the folders), for Japanese-mode pages on a Windows whose ANSI code page
//! is not Japanese. There the original draws its string resources through the ANSI code page
//! (Japanese characters become `?`, a full-width colon becomes `:`) and byte data (the texts
//! of t_ecaps12c.dll, the roman-numeral lead names) as Shift-JIS. Each non-ASCII string of
//! ours is drawn both ways and the one closer to the original is kept; the page is still
//! compared strictly. Byte data is thereby checked character by character; of the resource
//! strings only the number and place of the Japanese characters (research/recheck has the Wine
//! trace that checks the characters). `--dump FILE` (after --qmark, if both) writes every text
//! call (page, format, rectangle, bytes in hex) of the last folder.

#[cfg(not(windows))]
fn main() {
    eprintln!("gdi_check needs Windows GDI");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    win::main()
}

#[cfg(windows)]
mod win {
    use ecg_paint::analtext::AnalTables;
    use ecg_paint::convert;
    use ecg_paint::render::text::Font;
    use ecg_paint::render::RectI;
    use ecg_paint::settings::{self, Flags, ImageFormat, Ini};
    use std::ffi::c_void;
    use std::path::Path;

    type Handle = *mut c_void;

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        size_image: u32,
        x_ppm: i32,
        y_ppm: i32,
        clr_used: u32,
        clr_important: u32,
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn CreateDCA(driver: *const u8, device: *const u8, output: *const u8, devmode: *const c_void) -> Handle;
        fn CreateCompatibleDC(hdc: Handle) -> Handle;
        fn CreateCompatibleBitmap(hdc: Handle, w: i32, h: i32) -> Handle;
        fn SelectObject(hdc: Handle, obj: Handle) -> Handle;
        #[allow(clippy::too_many_arguments)]
        fn CreateFontA(
            h: i32,
            w: i32,
            esc: i32,
            orient: i32,
            weight: i32,
            italic: u32,
            underline: u32,
            strike: u32,
            charset: u32,
            out_prec: u32,
            clip_prec: u32,
            quality: u32,
            pitch: u32,
            face: *const u8,
        ) -> Handle;
        fn SetBkMode(hdc: Handle, mode: i32) -> i32;
        fn SetBkColor(hdc: Handle, c: u32) -> u32;
        fn SetTextColor(hdc: Handle, c: u32) -> u32;
        fn SetDIBits(
            hdc: Handle,
            hbm: Handle,
            start: u32,
            lines: u32,
            bits: *const c_void,
            bmi: *const BitmapInfoHeader,
            usage: u32,
        ) -> i32;
        fn GetDIBits(
            hdc: Handle,
            hbm: Handle,
            start: u32,
            lines: u32,
            bits: *mut c_void,
            bmi: *mut BitmapInfoHeader,
            usage: u32,
        ) -> i32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn DrawTextA(hdc: Handle, s: *const u8, n: i32, rc: *mut Rect, fmt: u32) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        #[allow(clippy::too_many_arguments)]
        fn WideCharToMultiByte(
            cp: u32,
            flags: u32,
            wide: *const u16,
            n: i32,
            out: *mut u8,
            cap: i32,
            def: *const u8,
            used_def: *mut i32,
        ) -> i32;
    }

    /// Unicode → the ANSI code page of this Windows, as LoadStringA and friends do it (best fit,
    /// `?` for the rest).
    fn to_ansi(s: &str) -> Vec<u8> {
        let wide: Vec<u16> = s.encode_utf16().collect();
        if wide.is_empty() {
            return Vec::new();
        }
        let mut out = vec![0u8; wide.len() * 4];
        let n = unsafe {
            WideCharToMultiByte(
                0,
                0,
                wide.as_ptr(),
                wide.len() as i32,
                out.as_mut_ptr(),
                out.len() as i32,
                std::ptr::null(),
                std::ptr::null_mut(),
            )
        };
        out.truncate(n.max(0) as usize);
        out
    }

    /// The original's memory DC: display-compatible bitmap, its font, transparent black text.
    struct Gdi {
        screen: Handle,
        mem: Handle,
        bmp: Handle,
        w: usize,
        h: usize,
    }

    impl Gdi {
        fn new(w: usize, h: usize, font_h: i32, font_w: i32, japanese: bool, resolution: u32) -> Gdi {
            unsafe {
                let screen = CreateDCA(c"DISPLAY".as_ptr().cast(), std::ptr::null(), std::ptr::null(), std::ptr::null());
                assert!(!screen.is_null(), "CreateDC(DISPLAY) failed");
                let mem = CreateCompatibleDC(screen);
                let bmp = CreateCompatibleBitmap(screen, w as i32, h as i32);
                assert!(!mem.is_null() && !bmp.is_null(), "memory DC / bitmap");
                // Init 0x407940 (0x4079c3…0x407ac0)
                let (face, charset, weight): (&[u8], u32, i32) = if japanese {
                    (b"MS\x83S\x83V\x83b\x83N\0", 0x80, if resolution == 2 { 600 } else { 500 })
                } else {
                    (b"Arial\0", 0, 500)
                };
                let font = CreateFontA(font_h, font_w, 0, 0, weight, 0, 0, 0, charset, 4, 0, 0, 0x21, face.as_ptr());
                assert!(!font.is_null(), "CreateFont");
                SelectObject(mem, font);
                SetBkColor(mem, 0x00ff_ffff);
                SetBkMode(mem, 1);
                SetTextColor(mem, 0);
                Gdi { screen, mem, bmp, w, h }
            }
        }

        fn header(&self) -> BitmapInfoHeader {
            BitmapInfoHeader {
                size: 40,
                width: self.w as i32,
                height: -(self.h as i32),
                planes: 1,
                bit_count: 24,
                compression: 0,
                size_image: 0,
                x_ppm: 0,
                y_ppm: 0,
                clr_used: 0,
                clr_important: 0,
            }
        }

        /// The page goes into the bitmap, GDI draws the text on it, the bitmap comes back.
        fn draw_text(&self, px: &mut [u8], s: &[u8], r: RectI, fmt: u32) {
            let stride = (self.w * 3 + 3) & !3;
            let mut bits = vec![0u8; stride * self.h];
            for y in 0..self.h {
                for x in 0..self.w {
                    let (i, o) = ((y * self.w + x) * 3, y * stride + x * 3);
                    bits[o] = px[i + 2];
                    bits[o + 1] = px[i + 1];
                    bits[o + 2] = px[i];
                }
            }
            let mut rc = Rect { left: r.l, top: r.t, right: r.r, bottom: r.b };
            unsafe {
                let mut bmi = self.header();
                let n = SetDIBits(self.screen, self.bmp, 0, self.h as u32, bits.as_ptr().cast(), &bmi, 0);
                assert_eq!(n, self.h as i32, "SetDIBits");
                let old = SelectObject(self.mem, self.bmp);
                DrawTextA(self.mem, s.as_ptr(), s.len() as i32, &mut rc, fmt);
                SelectObject(self.mem, old);
                let n = GetDIBits(self.screen, self.bmp, 0, self.h as u32, bits.as_mut_ptr().cast(), &mut bmi, 0);
                assert_eq!(n, self.h as i32, "GetDIBits");
            }
            for y in 0..self.h {
                for x in 0..self.w {
                    let (i, o) = ((y * self.w + x) * 3, y * stride + x * 3);
                    px[i] = bits[o + 2];
                    px[i + 1] = bits[o + 1];
                    px[i + 2] = bits[o];
                }
            }
        }
    }

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

    /// Uncompressed 24/32-bit BMP → RGB, top row first.
    fn decode_bmp(path: &Path) -> Option<(usize, usize, Vec<u8>)> {
        let d = std::fs::read(path).ok()?;
        if d.len() < 54 || &d[..2] != b"BM" {
            return None;
        }
        let u32at = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let off = u32at(10) as usize;
        let (w, h) = (u32at(18) as i32, u32at(22) as i32);
        let bpp = u16::from_le_bytes(d[28..30].try_into().unwrap()) as usize;
        if u32at(30) != 0 || !(bpp == 24 || bpp == 32) || w <= 0 || h == 0 {
            return None;
        }
        let (w, rows) = (w as usize, h.unsigned_abs() as usize);
        let stride = (w * bpp / 8 + 3) & !3;
        let mut out = vec![0u8; w * rows * 3];
        for y in 0..rows {
            let src = off + stride * if h > 0 { rows - 1 - y } else { y };
            for x in 0..w {
                let p = d.get(src + x * bpp / 8..src + x * bpp / 8 + 3)?;
                out[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[p[2], p[1], p[0]]);
            }
        }
        Some((w, rows, out))
    }

    /// One page of the original, decoded (None: not written, e.g. after its crash on short records).
    #[allow(clippy::type_complexity)]
    fn gold_page(dir: &Path, page_name: &str, format: ImageFormat) -> Result<Option<(usize, usize, Vec<u8>)>, String> {
        let ext = format.ext();
        let gold = std::fs::read_dir(dir.join("orig")).ok().and_then(|rd| {
            rd.flatten().map(|e| e.path()).find(|p| p.to_string_lossy().ends_with(&format!("_{page_name}{ext}")))
        });
        let Some(gold) = gold else { return Ok(None) };
        let decoded = if format == ImageFormat::Png { decode_png(&gold) } else { decode_bmp(&gold) };
        decoded.map(Some).ok_or_else(|| format!("cannot decode {}", gold.display()))
    }

    /// Pixels of `px` inside `r` that differ from `gold`.
    fn rect_diff(px: &[u8], gold: &[u8], w: usize, h: usize, r: RectI) -> usize {
        let mut n = 0;
        for y in r.t.max(0)..r.b.min(h as i32) {
            for x in r.l.max(0)..r.r.min(w as i32) {
                let i = (y as usize * w + x as usize) * 3;
                n += (px[i..i + 3] != gold[i..i + 3]) as usize;
            }
        }
        n
    }

    pub fn main() {
        let mut args: Vec<String> = std::env::args().skip(1).collect();
        let qmark = args.first().is_some_and(|a| a == "--qmark");
        if qmark {
            args.remove(0);
        }
        let dump = if args.first().is_some_and(|a| a == "--dump") {
            args.remove(0);
            Some(args.remove(0))
        } else {
            None
        };
        let mut bad = 0;
        let (mut pages, mut total, mut raw_calls, mut ansi_calls) = (0usize, 0usize, 0usize, 0usize);
        let mut ansi_strings = std::collections::BTreeSet::new();
        let mut raw_strings = std::collections::BTreeSet::new();
        for arg in args {
            let dir = Path::new(&arg);
            let name = dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let ini = Ini::new(dir.join("rust.ini"));
            let mut flags = Flags::default();
            for f in std::fs::read_to_string(dir.join("flags.txt")).unwrap_or_default().split_whitespace() {
                let up = f.to_uppercase();
                if let Some(v) = up.strip_prefix("-H") {
                    flags.h_option = Some(settings::atoi(v));
                } else if up == "-T" {
                    flags.attach = true;
                }
            }
            let s = settings::load(&ini, &flags);
            if s.attach {
                println!("{name:20} -T: no images");
                continue;
            }
            if s.format == ImageFormat::Jpeg {
                println!("{name:20} JPEG: lossy, not compared here");
                continue;
            }
            let data = std::fs::read(dir.join("in.ecg")).expect("in.ecg");
            let tables =
                std::fs::read(dir.join("bin").join("t_ecaps12c.dll")).ok().and_then(|b| AnalTables::from_dll(&b).ok());
            let opts = ecg_paint::format::ParseOptions { english: !s.japanese };
            let rec = match ecg_paint::format::parse(&data, opts, tables.as_ref()) {
                Ok(r) => r,
                Err(e) => {
                    println!("{name:20} parse error: {e}");
                    bad += 1;
                    continue;
                }
            };
            let prep = convert::prepare(&rec, &s);
            let lo = ecg_paint::render::layout::Layout::for_resolution(s.resolution);
            let (w, h) = (lo.w as usize, lo.h as usize);
            let mut dump_text = String::new();
            for (index, page_name) in ["AVE", "6CH"].into_iter().enumerate() {
                let gold = match gold_page(dir, page_name, s.format) {
                    Ok(Some(g)) => g,
                    Ok(None) => {
                        println!("{name:20} {page_name}: no original image (the original did not write one)");
                        continue;
                    }
                    Err(e) => {
                        println!("{name:20} {page_name}: {e}");
                        bad += 1;
                        continue;
                    }
                };
                if (gold.0, gold.1) != (w, h) {
                    println!("{name:20} {page_name}: size {}x{} vs ours {w}x{h}", gold.0, gold.1);
                    bad += 1;
                    continue;
                }
                let gold = std::rc::Rc::new(gold.2);
                // (format, rect, bytes, drawn through the ANSI code page) of every text call
                let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(u32, RectI, Vec<u8>, bool)>::new()));
                let gdi = Gdi::new(w, h, lo.font_h, lo.font_w, s.japanese, s.resolution);
                let font = Font::with_renderer(lo.font_h, lo.font_w, {
                    let (calls, gold) = (calls.clone(), gold.clone());
                    Box::new(move |px, _w, _h, text, r, fmt| {
                        let ansi = |b: &[u8]| to_ansi(&encoding_rs::SHIFT_JIS.decode(b).0);
                        if !qmark || ansi(text) == text {
                            gdi.draw_text(px, text, r, fmt);
                            calls.borrow_mut().push((fmt, r, text.to_vec(), false));
                            return;
                        }
                        // Japanese text on a Windows with another ANSI code page (see the top of
                        // this file). A string can be a label (through the code page) followed
                        // by data (Shift-JIS bytes) or the reverse, so every split next to a
                        // Japanese character is drawn; the one closest to the original inside
                        // the rectangle is kept and the whole page is compared strictly later.
                        let mut cuts = vec![0];
                        let mut i = 0;
                        while i < text.len() {
                            let lead = matches!(text[i], 0x81..=0x9F | 0xE0..=0xFC) && i + 1 < text.len();
                            let step = if lead { 2 } else { 1 };
                            let (here, next) = (text[i] >= 0x80, text.get(i + step).is_some_and(|b| *b >= 0x80));
                            if here || next {
                                cuts.push(i + step);
                            }
                            i += step;
                        }
                        cuts.push(text.len());
                        cuts.dedup();
                        let mut candidates: Vec<(Vec<u8>, bool)> = Vec::new();
                        for &k in &cuts {
                            candidates.push(([ansi(&text[..k]), text[k..].to_vec()].concat(), k > 0));
                            candidates.push(([text[..k].to_vec(), ansi(&text[k..])].concat(), k < text.len()));
                        }
                        let before = px.to_vec();
                        let mut best: Option<(usize, Vec<u8>, bool)> = None;
                        for (bytes, through_ansi) in candidates {
                            let mut page = before.clone();
                            gdi.draw_text(&mut page, &bytes, r, fmt);
                            let d = rect_diff(&page, &gold, w, h, r);
                            if best.as_ref().map_or(true, |b| d < b.0) {
                                best = Some((d, page, through_ansi));
                            }
                        }
                        let (_, page, through_ansi) = best.unwrap();
                        px.copy_from_slice(&page);
                        calls.borrow_mut().push((fmt, r, text.to_vec(), through_ansi));
                    })
                })
                .unwrap();
                let (ave, six) = convert::render_pages(&rec, &s, &prep, &ini, &font, settings::rhythm_lead(&ini));
                // both pages get drawn; the calls of the page under test are the first / last ones
                let page = if index == 0 { &ave } else { &six };
                let all = calls.borrow();
                let mine = if index == 0 { &all[..ave.text_rects.len()] } else { &all[ave.text_rects.len()..] };
                for (fmt, r, text, through_ansi) in mine {
                    let hex: String = text.iter().map(|b| format!("{b:02x}")).collect();
                    dump_text.push_str(&format!("{page_name} {fmt:x} {} {} {} {} {hex}\n", r.l, r.t, r.r, r.b));
                    if !text.is_ascii() {
                        let shown = encoding_rs::SHIFT_JIS.decode(text).0.into_owned();
                        if *through_ansi {
                            ansi_calls += 1;
                            ansi_strings.insert(shown);
                        } else {
                            raw_calls += 1;
                            raw_strings.insert(shown);
                        }
                    }
                }
                let mut n = 0usize;
                let mut img = vec![255u8; w * h * 3];
                for i in 0..w * h {
                    let (a, b) = (&page.rgb[i * 3..i * 3 + 3], &gold[i * 3..i * 3 + 3]);
                    let px: [u8; 3] = if a == b {
                        if a == [255, 255, 255] { [255, 255, 255] } else { [200, 200, 200] }
                    } else {
                        n += 1;
                        if a == [255, 255, 255] { [255, 0, 0] } else { [0, 0, 255] }
                    };
                    img[i * 3..i * 3 + 3].copy_from_slice(&px);
                }
                pages += 1;
                total += n;
                println!("{name:20} {page_name}: {n:6} of {} pixels differ, {} text calls", w * h, mine.len());
                if n > 0 {
                    bad += 1;
                    let _ = std::fs::write(dir.join(format!("gdidiff_{page_name}.png")), ecg_paint::encode::png(&img, w, h, 6));
                    let _ = std::fs::write(dir.join(format!("gdi_{page_name}.png")), ecg_paint::encode::png(&page.rgb, w, h, 6));
                }
            }
            if let Some(path) = &dump {
                std::fs::write(path, &dump_text).expect("dump");
            }
        }
        println!("{pages} pages compared, {total} differing pixels in total");
        if qmark {
            println!("non-ASCII text calls: {raw_calls} matched as Shift-JIS bytes, {ansi_calls} through the ANSI code page");
            let mut list = String::from("# drawn as Shift-JIS bytes by the original\n");
            for t in &raw_strings {
                list.push_str(&format!("{t}\n"));
            }
            list.push_str("# drawn through the ANSI code page by the original (string resources)\n");
            for t in &ansi_strings {
                list.push_str(&format!("{t}\n"));
            }
            let _ = std::fs::write("gdi_check_strings.txt", list);
            println!("the strings of both kinds: gdi_check_strings.txt (UTF-8, in the current folder)");
        }
        if bad > 0 {
            std::process::exit(1);
        }
    }
}
