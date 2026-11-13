//! Damaged input must give an error or a picture, never a panic or a hang.

use ecg_paint::convert;
use ecg_paint::format::{parse, ParseOptions};
use ecg_paint::render::text::Font;
use ecg_paint::settings::{self, Flags, Ini};

const INPUT: &[u8] = include_bytes!("golden/synth_m0/synth_rest_m0.ecg");

/// xorshift, deterministic.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Recompute every block checksum so damage reaches the block parsers and the decoder.
fn reseal(buf: &mut [u8]) {
    let be16 = |b: &[u8], o: usize| u16::from_be_bytes([b[o], b[o + 1]]) as usize;
    let n = be16(buf, 0x10).min(64);
    for i in 0..n {
        let o = 0x12 + 6 * i;
        if o + 6 > buf.len() {
            break;
        }
        let off = u32::from_be_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]) as usize + 6;
        if off + 6 > buf.len() {
            continue;
        }
        let len = u32::from_be_bytes([buf[off + 2], buf[off + 3], buf[off + 4], buf[off + 5]]) as usize;
        if off + 2 + len > buf.len() {
            continue;
        }
        let s = buf[off + 2..off + 2 + len].iter().fold(0u16, |s, &x| s.wrapping_add(x as u16));
        buf[off..off + 2].copy_from_slice(&s.to_be_bytes());
    }
}

#[test]
fn mutated_files_never_panic() {
    let tmp = std::env::temp_dir().join(format!("ecg-paint-robust-{}.ini", std::process::id()));
    std::fs::write(&tmp, include_bytes!("golden/synth_m0/ecgviewer.ini")).unwrap();
    let ini = Ini::new(&tmp);
    let s = settings::load(&ini, &Flags::default());
    let font = Font::new(14, 7, None).unwrap();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut rendered = 0;
    for case in 0..400 {
        let mut b = INPUT.to_vec();
        match case % 4 {
            0 => b.truncate(rng.next() as usize % b.len()),
            _ => {
                for _ in 0..1 + rng.next() % 40 {
                    let i = rng.next() as usize % b.len();
                    b[i] = rng.next() as u8;
                }
            }
        }
        if case % 4 >= 2 {
            reseal(&mut b);
        }
        if let Ok(rec) = parse(&b, ParseOptions { english: true }, None) {
            let prep = convert::prepare(&rec, &s);
            if rendered < 12 {
                let _ = convert::render_pages(&rec, &s, &prep, &ini, &font, Some(1));
                rendered += 1;
            }
            let _ = ecg_paint::txt::render(
                &rec,
                &ecg_paint::txt::TxtOptions { japanese: case % 2 == 0, show_id: true, rhythm_lead: None, h_option: None },
            );
        }
    }
    let _ = std::fs::remove_file(&tmp);
}
