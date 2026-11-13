//! DrawText with a bundled open font. The original asks GDI for Arial (English) or
//! "MSゴシック" (Japanese) at the cell size of the image size; neither can be shipped, so the
//! glyphs are Liberation Mono (SIL OFL), on the same fixed character grid (one cell width per
//! byte), aliased black. Text pixels therefore differ from the original's; positions, rects,
//! alignment and clipping follow DrawText.

use super::{Canvas, RectI, BLACK, DT_RIGHT, DT_WORDBREAK};
use std::cell::RefCell;
use std::collections::HashMap;

static LIBERATION_MONO: &[u8] = include_bytes!("../../assets/LiberationMono-Regular.ttf");

type Glyph = (fontdue::Metrics, Vec<u8>);

/// Draws one DrawText call straight into the page: (RGB pixels, width, height, text bytes, rect,
/// DrawText format). Verification tooling uses it to let Windows GDI draw the text with the
/// original's font, so whole images can be compared with the original's (examples/gdi_check.rs).
pub type TextRenderer = Box<dyn Fn(&mut [u8], usize, usize, &[u8], RectI, u32)>;

pub struct Font {
    faces: Vec<fontdue::Font>,
    /// Cell height and width (CreateFont height / width).
    pub cell_h: i32,
    pub cell_w: i32,
    em: f32,
    ascent: i32,
    cache: RefCell<HashMap<char, Option<Glyph>>>,
    renderer: Option<TextRenderer>,
}

impl Font {
    /// The bundled font, plus an optional extra font file for characters it lacks (Japanese).
    pub fn new(cell_h: i32, cell_w: i32, extra: Option<&[u8]>) -> Result<Font, String> {
        let settings = fontdue::FontSettings::default();
        let mut faces = vec![fontdue::Font::from_bytes(LIBERATION_MONO, settings)?];
        if let Some(b) = extra {
            faces.push(fontdue::Font::from_bytes(b, fontdue::FontSettings::default())?);
        }
        // cell height = ascent + descent of the em
        let m = faces[0].horizontal_line_metrics(1.0).ok_or("font has no line metrics")?;
        let em = cell_h as f32 / (m.ascent - m.descent);
        let ascent = (m.ascent * em).round() as i32;
        Ok(Font { faces, cell_h, cell_w, em, ascent, cache: RefCell::new(HashMap::new()), renderer: None })
    }

    /// A font whose text is drawn by `renderer` instead of the bundled glyphs.
    pub fn with_renderer(cell_h: i32, cell_w: i32, renderer: TextRenderer) -> Result<Font, String> {
        let mut f = Font::new(cell_h, cell_w, None)?;
        f.renderer = Some(renderer);
        Ok(f)
    }

    fn glyph(&self, ch: char) -> Option<Glyph> {
        if let Some(g) = self.cache.borrow().get(&ch) {
            return g.clone();
        }
        let g = self
            .faces
            .iter()
            .find(|f| f.lookup_glyph_index(ch) != 0)
            .map(|f| f.rasterize(ch, self.em));
        self.cache.borrow_mut().insert(ch, g.clone());
        g
    }

    /// Split Shift-JIS bytes into (char, cells) units.
    fn units(s: &[u8]) -> Vec<(char, i32)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < s.len() {
            let b = s[i];
            let lead = (0x81..=0x9F).contains(&b) || (0xE0..=0xFC).contains(&b);
            if lead && i + 1 < s.len() {
                let (txt, _, _) = encoding_rs::SHIFT_JIS.decode(&s[i..i + 2]);
                out.push((txt.chars().next().unwrap_or(' '), 2));
                i += 2;
            } else {
                let (txt, _, _) = encoding_rs::SHIFT_JIS.decode(&s[i..i + 1]);
                out.push((txt.chars().next().unwrap_or(' '), 1));
                i += 1;
            }
        }
        out
    }

    /// Lines as DrawText lays them out: CR/LF breaks; with DT_WORDBREAK, words wrapped to the
    /// rect width.
    fn lines(&self, s: &[u8], width: i32, wordbreak: bool) -> Vec<Vec<(char, i32)>> {
        let mut out = Vec::new();
        for raw in s.split(|&c| c == b'\n') {
            let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
            let units = Self::units(raw);
            if !wordbreak {
                out.push(units);
                continue;
            }
            let mut line: Vec<(char, i32)> = Vec::new();
            let mut i = 0;
            while i < units.len() {
                // next word (with its trailing spaces)
                let mut j = i;
                while j < units.len() && units[j].0 != ' ' {
                    j += 1;
                }
                while j < units.len() && units[j].0 == ' ' {
                    j += 1;
                }
                let word = &units[i..j];
                let cur: i32 = line.iter().map(|u| u.1).sum::<i32>() * self.cell_w;
                let ww: i32 = word.iter().filter(|u| u.0 != ' ').map(|u| u.1).sum::<i32>() * self.cell_w;
                if !line.is_empty() && cur + ww > width {
                    out.push(std::mem::take(&mut line));
                }
                line.extend_from_slice(word);
                i = j.max(i + 1);
            }
            out.push(line);
        }
        out
    }

    pub fn draw(&self, c: &mut Canvas, s: &[u8], r: RectI, fmt: u32) {
        if let Some(render) = &self.renderer {
            let (w, h) = (c.w, c.h);
            render(&mut c.px, w, h, s, r, fmt);
            return;
        }
        let lines = self.lines(s, r.r - r.l, fmt & DT_WORDBREAK != 0);
        for (n, line) in lines.iter().enumerate() {
            let top = r.t + n as i32 * self.cell_h;
            if top >= r.b {
                break;
            }
            let width: i32 = line.iter().map(|u| u.1).sum::<i32>() * self.cell_w;
            let mut x = if fmt & DT_RIGHT != 0 { r.r - width } else { r.l };
            let base = top + self.ascent;
            for &(ch, cells) in line {
                if let Some((m, bmp)) = self.glyph(ch) {
                    let ox = x + m.xmin;
                    let oy = base - m.height as i32 - m.ymin;
                    for gy in 0..m.height {
                        for gx in 0..m.width {
                            if bmp[gy * m.width + gx] >= 128 {
                                let (px, py) = (ox + gx as i32, oy + gy as i32);
                                if px >= r.l && px < r.r && py >= r.t && py < r.b {
                                    c.put(px, py, BLACK);
                                }
                            }
                        }
                    }
                }
                x += cells * self.cell_w;
            }
        }
    }
}
