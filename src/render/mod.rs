//! Record → pixels. A tiny GDI look-alike: aliased 1-px lines with GDI's end-point rules,
//! Rectangle with a NULL brush, DrawText with clipping. Every drawing call is also logged
//! in the same compact form as the GDI trace of the original (tests compare both).

pub mod ave;
pub mod layout;
pub mod sixch;
pub mod text;
pub mod wave;

use text::Font;

pub type Rgb = [u8; 3];
pub const BLACK: Rgb = [0, 0, 0];
pub const WHITE: Rgb = [255, 255, 255];

/// DrawText flags used by EcgPaint.
pub const DT_LEFT: u32 = 0;
pub const DT_RIGHT: u32 = 2;
pub const DT_WORDBREAK: u32 = 0x10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RectI {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

pub struct Canvas<'f> {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
    pen: Rgb,
    pen_width: i32,
    font: &'f Font,
    /// The drawing calls, one per line ("RECT …", "PEN …", "POLY n", "SEG x y1 y2", "fmt \"text\"").
    pub log: Vec<String>,
    /// Rectangles of every DrawText call (text pixels are excluded from golden comparisons).
    pub text_rects: Vec<RectI>,
}

impl<'f> Canvas<'f> {
    /// A white page (0x409b40: FillSolidRect white, black pen, NULL brush).
    pub fn new(w: usize, h: usize, font: &'f Font) -> Self {
        Canvas {
            w,
            h,
            px: vec![255; w * h * 3],
            pen: BLACK,
            pen_width: 1,
            font,
            log: Vec::new(),
            text_rects: Vec::new(),
        }
    }

    pub fn put(&mut self, x: i32, y: i32, c: Rgb) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            let i = (y as usize * self.w + x as usize) * 3;
            self.px[i..i + 3].copy_from_slice(&c);
        }
    }

    /// GDI's width-2 pen (IMGDENSITY=1: the grid pen, the only wide pen EcgPaint uses) on a
    /// horizontal or vertical segment, as measured pixel by pixel from EcgPaint.exe running on
    /// Windows 11 (Wine draws these differently, so the Wine trace is no reference here):
    ///   horizontal, either direction: row y gets x_min..=x_max, row y-1 gets x_min+1..=x_max
    ///     (a grid dot (x,y)->(x+1,y) becomes (x,y), (x+1,y), (x+1,y-1));
    ///   vertical, either direction: columns x-1 and x.
    /// Only grid dots and the four frame lines are drawn this way. The left frame line looks
    /// three pixels wide at 800x600 and 1024x768 because the grid dots that start on it fill the
    /// column to its right on every row; at 1280x1024 (4.8 px/mm) those dots leave gaps there,
    /// as in the original.
    fn wide_axis_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let c = self.pen;
        if y0 == y1 {
            let (a, b) = (x0.min(x1), x0.max(x1));
            for x in a..=b {
                self.put(x, y0, c);
            }
            for x in a + 1..=b {
                self.put(x, y0 - 1, c);
            }
        } else {
            for y in y0.min(y1)..=y0.max(y1) {
                self.put(x0 - 1, y, c);
                self.put(x0, y, c);
            }
        }
    }

    /// CreatePen(PS_SOLID, width, colour) + SelectObject. The stock BLACK_PEN selection is not logged.
    pub fn create_pen(&mut self, width: i32, c: Rgb) {
        self.log.push(format!("PEN {} {:02x}{:02x}{:02x}", width, c[2], c[1], c[0]));
        self.select_pen(width, c);
    }

    /// The selected pen (to restore it after a temporary one, like SelectObject's return value).
    pub fn pen(&self) -> (i32, Rgb) {
        (self.pen_width, self.pen)
    }

    pub fn select_pen(&mut self, width: i32, c: Rgb) {
        self.pen = c;
        self.pen_width = width;
    }

    /// A line from (x0,y0) to (x1,y1), last point excluded (Bresenham, like GDI's cosmetic pens).
    /// Wide pens are only ever used for the axis-aligned grid; a wide diagonal line (unreachable)
    /// would be drawn 1 px wide.
    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        if self.pen_width >= 2 && (x0 == x1 || y0 == y1) && (x0, y0) != (x1, y1) {
            self.wide_axis_line(x0, y0, x1, y1);
            return;
        }
        let c = self.pen;
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        while !(x == x1 && y == y1) {
            self.put(x, y, c);
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn polyline(&mut self, pts: &[(i32, i32)]) {
        self.log.push(format!("POLY {}", pts.len()));
        for w in pts.windows(2) {
            self.line(w[0].0, w[0].1, w[1].0, w[1].1);
        }
    }

    /// MoveToEx(x, y1) + LineTo(x, y2): a vertical run of the wave.
    pub fn segment(&mut self, x: i32, y1: i32, y2: i32) {
        self.log.push(format!("SEG {x} {y1} {y2}"));
        self.line(x, y1, x, y2);
    }

    /// Rectangle with a NULL brush: outline of l..r-1 × t..b-1.
    pub fn rectangle(&mut self, r: RectI) {
        self.log.push(format!("RECT {} {} {} {}", r.l, r.t, r.r, r.b));
        let (l, t, rr, bb) = (r.l, r.t, r.r - 1, r.b - 1);
        self.line(l, t, rr, t);
        self.line(rr, t, rr, bb);
        self.line(rr, bb, l, bb);
        self.line(l, bb, l, t);
    }

    /// DrawText(text, rect, fmt) in black, clipped to the rect.
    pub fn draw_text(&mut self, s: &[u8], r: RectI, fmt: u32) {
        let shown = String::from_utf8_lossy(s);
        self.log.push(format!("{:x} \"{}\"", fmt, shown));
        self.text_rects.push(r);
        let font = self.font;
        font.draw(self, s, r, fmt);
    }

}

/// The two pages and the shapes they are drawn with.
pub struct Pages {
    pub ave: Page,
    pub six: Page,
}

pub struct Page {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u8>,
    pub log: Vec<String>,
    pub text_rects: Vec<RectI>,
}

impl From<Canvas<'_>> for Page {
    fn from(c: Canvas<'_>) -> Self {
        Page { w: c.w, h: c.h, rgb: c.px, log: c.log, text_rects: c.text_rects }
    }
}

/// C `_ftol`: truncation toward zero.
pub fn t(x: f64) -> i32 {
    if x.is_nan() {
        0
    } else {
        x.trunc().clamp(i32::MIN as f64, i32::MAX as f64) as i32
    }
}

/// Run-length view of a log (consecutive equal lines folded into "line xN"), the format of
/// tests/golden/*/prims_*.txt.
pub fn compact_log(log: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < log.len() {
        let mut j = i;
        while j < log.len() && log[j] == log[i] {
            j += 1;
        }
        out.push(if j - i > 1 { format!("{} x{}", log[i], j - i) } else { log[i].clone() });
        i = j;
    }
    out
}
