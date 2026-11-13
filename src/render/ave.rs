//! The `_AVE` page (0x409d30): patient/measurement text on the left, the 12 average beats
//! (3 rows × 4 columns, 0x4063b0) on a grid at the top right, a 10 s rhythm strip of the
//! RhythmLead (0x406a90) at the bottom.

use super::layout::Layout;
use super::wave::{self, sample};
use super::{t, Canvas, RectI, DT_LEFT, DT_RIGHT, DT_WORDBREAK};
use super::{BLACK, Rgb};
use crate::fields::{self, Ctx};
use crate::model::{lead_name, Record, DISP2STORE};

pub struct PageInput<'a> {
    pub rec: &'a Record,
    /// Filtered records (wave+0x19c).
    pub filtered: &'a [[i16; 16]],
    pub shifts: [i32; 12],
    pub centres: [i32; 12],
    pub japanese: bool,
    pub show_id: bool,
    pub grid: Rgb,
    pub grid_width: i32,
    pub rhythm_lead: Option<usize>,
    pub h_option: Option<i32>,
}

fn rect(l: i32, t: i32, r: i32, b: i32) -> RectI {
    RectI { l, t, r, b }
}

/// 0x40c620: grid of 1 mm dots, 5 mm dotted lines (every 2 px), and dots every 5 mm on the
/// half-millimetre rows (which make the dotted vertical 5 mm lines), inside a solid border.
pub fn grid(c: &mut Canvas, l: i32, top: i32, r: i32, b: i32, px: f64) {
    c.polyline(&[(l, top), (r, top)]);
    c.polyline(&[(r, b), (r, top)]);
    c.polyline(&[(r, b), (l, b)]);
    c.polyline(&[(l, top), (l, b)]);
    let topd = top as f64;
    let mut k = 0i32;
    while topd + k as f64 * px < b as f64 {
        let kmm = k as f64 * px;
        let y = t(topd + kmm);
        let dot = |c: &mut Canvas, x: i32, y: i32| c.polyline(&[(x, y), (x + 1, y)]);
        if k % 5 == 0 {
            let mut x = l;
            while x < r {
                dot(c, x, y);
                x += 2;
            }
        } else {
            // dots at l + trunc(j·mm)
            let mut x = l;
            let mut j = 0i32;
            while x < r {
                dot(c, x, y);
                x = t(j as f64 * px - 1.0 + (l + 1) as f64);
                j += 1;
            }
        }
        // half-mm row: dots every 5 mm
        let y2 = t(px * 0.5 + topd + kmm);
        let mut x = l;
        let mut j = 0i32;
        while x < r {
            dot(c, x, y2);
            x = t(j as f64 * px * 5.0 - 1.0 + (l + 1) as f64);
            j += 1;
        }
        k += 1;
    }
}

/// Six-point calibration pulse (1 mV, 0.2 s at the left of a strip).
pub fn cal_pulse(c: &mut Canvas, xs: [i32; 4], base: i32, top: i32) {
    c.polyline(&[(xs[0], base), (xs[1], base), (xs[1], top), (xs[2], top), (xs[2], base), (xs[3], base)]);
}

pub fn draw(c: &mut Canvas, lo: &Layout, p: &PageInput) {
    let r = p.rec;
    let ctx = Ctx { japanese: p.japanese };
    let (w, h) = (lo.w, lo.h);
    let dy = 2 * h / 100;
    c.rectangle(rect((w / 200) / 2, h / 100, w - (w / 200) / 2, h - h / 100));
    c.create_pen(p.grid_width, p.grid);

    // left text block: rect A (labels) and rect B (values), one row per dy
    let (al, ar) = (lo.wp(2), lo.wp(31));
    let (bl, br) = (3 * lo.wp(31) / 8, 9 * lo.wp(31) / 10);
    let row = |n: i32| (dy + n * dy, 2 * dy + n * dy);
    let a = |n: i32| {
        let (t, b) = row(n);
        rect(al, t, ar, b)
    };
    let b = |n: i32| {
        let (t, b) = row(n);
        rect(bl, t, br, b)
    };
    let pt = &r.patient;
    c.draw_text(&ctx.s(133), a(0), DT_LEFT);
    if p.show_id {
        c.draw_text(&pt.id, b(0), DT_LEFT);
    }
    c.draw_text(&ctx.s(134), a(1), DT_LEFT);
    if p.show_id {
        c.draw_text(&pt.name, b(1), DT_LEFT);
    }
    c.draw_text(&fields::sex(ctx, r), a(2), DT_LEFT);
    c.draw_text(&fields::age(ctx, r), b(2), DT_LEFT);
    c.draw_text(&fields::height(r), a(3), DT_LEFT);
    c.draw_text(&fields::weight(r), b(3), DT_LEFT);
    c.draw_text(&fields::blood_pressure(r), b(3), DT_RIGHT);
    c.draw_text(&ctx.s(150), a(4), DT_LEFT);
    c.draw_text(&fields::medication(ctx, r, 1), b(4), DT_LEFT);
    c.draw_text(&fields::medication(ctx, r, 2), b(5), DT_LEFT);
    for (i, (id, v)) in fields::measurement_lines(r).into_iter().enumerate() {
        c.draw_text(&ctx.s(id), a(6 + i as i32), DT_LEFT);
        c.draw_text(&v, b(6 + i as i32), DT_RIGHT);
    }
    c.draw_text(&ctx.s(160), a(13), DT_LEFT);
    c.draw_text(&pt.technician, b(13), DT_LEFT);
    c.draw_text(&ctx.s(159), a(14), DT_LEFT);
    c.draw_text(&pt.req_doctor, b(14), DT_LEFT);

    // statements, then the comment block down to the "overread" line
    let sr = 9 * lo.wp(48) / 10;
    let (mut top, _) = row(15);
    for line in fields::statement_lines(r, !p.japanese, true) {
        top += dy;
        c.draw_text(&line, rect(al, top, sr, top + dy), DT_LEFT);
    }
    let (comment, edited) = fields::comment(r, p.japanese);
    let foot = lo.hp(74);
    c.draw_text(&comment, rect(al, top + 2 * dy, sr, foot - 2 * dy), DT_WORDBREAK);
    let over = if edited {
        [&ctx.s(131)[..], &fields::date(&r.analysis.edit_date), b"  ", &ctx.s(132), &pt.doctor].concat()
    } else {
        [&ctx.s(140)[..], b"  ", &ctx.s(132)].concat()
    };
    c.draw_text(&over, rect(al, foot - 2 * dy, sr, foot - dy), DT_LEFT);
    c.draw_text(&fields::device_line(r), rect(lo.wp(2), lo.hp(99) - dy, lo.wp(80), lo.hp(99)), DT_LEFT);
    c.draw_text(&fields::exam(ctx, r), rect(lo.wp(48), dy, lo.wp(80), 2 * dy), DT_LEFT);
    let mut mr = rect(lo.wp(31), 4 * dy, lo.wp(48), 5 * dy);
    c.draw_text(&fields::minnesota_head(ctx, r), mr, DT_LEFT);
    for e in &r.analysis.minnesota {
        mr.t += dy;
        mr.b += dy;
        c.draw_text(&fields::minnesota_code(*e), mr, DT_LEFT);
    }
    c.draw_text(&fields::date(&r.acquired), rect(lo.wp(80), dy, lo.wp(98), 2 * dy), DT_RIGHT);

    // average beats
    let (gl, gt) = (lo.wp(44), lo.hp(5));
    grid(c, gl, gt, t(lo.px * 145.0 + gl as f64), t(lo.px * 135.0 + gt as f64), lo.px);
    average_beats(c, lo, p, gl, gt);

    // rhythm strip
    let (rx, ry) = (lo.wp(1) + w / 200, lo.hp(74));
    grid(c, rx, ry, t(lo.px * 260.0 + lo.wp(1) as f64 + (w / 200) as f64), t(lo.px * 45.0 + ry as f64), lo.px);
    let lt = ry + lo.hp(1);
    c.draw_text(
        &fields::rhythm_label(r, p.japanese, p.rhythm_lead, p.h_option),
        rect(lo.wp(3) + lo.wp(2), lt, lo.wp(80), lt + dy),
        DT_LEFT,
    );
    rhythm_strip(c, lo, p, rx, ry);
}

/// 0x4063b0: calibration pulses, label, 12 average beats from the file's window x0..x1.
fn average_beats(c: &mut Canvas, lo: &Layout, p: &PageInput, left: i32, top: i32) {
    let px = lo.px;
    let old = c.pen();
    c.create_pen(1, BLACK);
    let (m4, m5, m8, m9) = (lo.mm(4.0), lo.mm(5.0), lo.mm(8.0), lo.mm(9.0));
    for k in 0..3 {
        let base = top + lo.mm(25.0) + k * lo.mm(45.0);
        let tp = top + lo.mm(15.0) + k * lo.mm(45.0);
        cal_pulse(c, [left + m4, left + m5, left + m8, left + m9], base, tp);
    }
    let (ll, lt) = (left + lo.mm(16.0), top + lo.mm(1.0));
    c.draw_text(b"10mm/mV 25mm/s Average", rect(ll, lt, ll + lo.mm(50.0), lt + lo.mm(10.0)), DT_LEFT);
    let wv = &p.rec.waves;
    let n = wv.x1.wrapping_sub(wv.x0) as i16;
    let spp = 20.0 / px;
    let scale = spp + spp;
    for (i, &store) in DISP2STORE.iter().enumerate() {
        let (col, row) = ((i / 3) as i32, (i % 3) as i32);
        let l = left + lo.mm(15.0) + col * lo.mm(31.0);
        let tp = top + lo.mm(10.0) + row * lo.mm(45.0);
        c.draw_text(&lead_name(i, p.japanese), rect(l, tp, l + lo.mm(15.0), tp + lo.mm(10.0)), DT_LEFT);
        if n > 0 {
            let start = 600 * store + wv.x0 as usize;
            let data: Vec<i16> = (0..n as usize).map(|j| wv.avg.get(start + j).copied().unwrap_or(0)).collect();
            wave::draw(c, &data, l, tp + lo.mm(15.0), spp, scale);
        }
    }
    c.select_pen(old.0, old.1);
}

/// 0x406a90: the first 10 s of the rhythm lead, shifted by wave+0x1a0.
fn rhythm_strip(c: &mut Canvas, lo: &Layout, p: &PageInput, x: i32, y: i32) {
    let disp = p.rhythm_lead.unwrap_or(1);
    let n = p.filtered.len().min(5000);
    let spp = 20.0 / lo.px;
    let scale = spp * 4.0;
    let shift = p.shifts[disp];
    let old = c.pen();
    c.create_pen(1, BLACK);
    let xs = [x + lo.mm(2.0), x + lo.mm(3.0), x + lo.mm(6.0), x + lo.mm(7.0)];
    cal_pulse(c, xs, y + lo.mm(25.0) - shift, y + lo.mm(15.0) - shift);
    let data: Vec<i16> = (0..n).map(|i| sample(p.filtered, disp, i)).collect();
    wave::draw(c, &data, x + lo.mm(10.0), y + lo.mm(25.0) - shift, spp, scale);
    c.select_pen(old.0, old.1);
}
