//! Image size, scale and font per IMGSIZE (Init 0x407940).

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub w: i32,
    pub h: i32,
    /// Pixels per millimetre (page+0x30).
    pub px: f64,
    /// CreateFont height and width.
    pub font_h: i32,
    pub font_w: i32,
}

impl Layout {
    pub fn for_resolution(res: u32) -> Layout {
        match res {
            0 => Layout { w: 800, h: 600, px: 3.0, font_h: 12, font_w: 6 },
            2 => Layout { w: 1280, h: 1024, px: 4.8, font_h: 18, font_w: 9 },
            _ => Layout { w: 1024, h: 768, px: 3.8, font_h: 14, font_w: 7 },
        }
    }

    /// p·W/100 (C integer division).
    pub fn wp(&self, p: i32) -> i32 {
        p * self.w / 100
    }

    pub fn hp(&self, p: i32) -> i32 {
        p * self.h / 100
    }

    /// trunc(k·px) — the original always computes t(−k·px) and subtracts it, which is the same.
    pub fn mm(&self, k: f64) -> i32 {
        super::t(self.px * k)
    }
}
