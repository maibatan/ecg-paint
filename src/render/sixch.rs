//! The `_6CH` page (0x40ba10): 12 leads in two columns of 5 s (limb leads samples 0..2499,
//! chest leads 2500..4999) on one grid (0x406e40), header and footer text.

use super::ave::{cal_pulse, grid, PageInput};
use super::layout::Layout;
use super::wave::{self, sample};
use super::{t, Canvas, RectI, BLACK, DT_LEFT, DT_RIGHT};
use crate::fields::{self, Ctx};
use crate::model::lead_name;

fn rect(l: i32, t: i32, r: i32, b: i32) -> RectI {
    RectI { l, t, r, b }
}

pub fn draw(c: &mut Canvas, lo: &Layout, p: &PageInput) {
    let r = p.rec;
    let ctx = Ctx { japanese: p.japanese };
    let (w, h) = (lo.w, lo.h);
    c.rectangle(rect(w / 200, h / 100, w - w / 200, h - h / 100));
    c.create_pen(p.grid_width, p.grid);
    let (gl, gt) = (lo.wp(1), lo.wp(7)); // the grid top is a share of the width
    grid(c, gl, gt, t(lo.px * 260.0 + gl as f64), t(lo.px * 175.0 + gt as f64), lo.px);
    waves(c, lo, p, gl, gt + lo.mm(5.0));

    let h2 = 2 * h / 100;
    let vr = 29 * lo.wp(48) / 30;
    let pt = &r.patient;
    let with = |label: Vec<u8>, v: &[u8]| {
        let mut s = label;
        s.push(b' ');
        if p.show_id {
            s.extend_from_slice(v);
        }
        s
    };
    c.draw_text(&with(ctx.s(133), &pt.id), rect(3 * lo.wp(31) / 4, 2 * h2, vr, 3 * h2), DT_LEFT);
    c.draw_text(&with(ctx.s(134), &pt.name), rect(lo.wp(2), 2 * h2, vr, 3 * h2), DT_LEFT);
    let sex_age = [&fields::sex(ctx, r)[..], b"   ", &fields::age(ctx, r)].concat();
    c.draw_text(&sex_age, rect(lo.wp(2), 3 * h2, vr, 4 * h2), DT_LEFT);
    let body = [
        &fields::height(r)[..],
        b" ",
        &fields::weight(r),
        b" ",
        &fields::blood_pressure(r),
        b" ",
        &fields::heart_rate(r),
    ]
    .concat();
    c.draw_text(&body, rect(lo.wp(2), 3 * h2, vr, 4 * h2), DT_RIGHT);
    c.draw_text(&ctx.s(150), rect(lo.wp(48), 2 * h2, lo.wp(80), 3 * h2), DT_LEFT);
    let ml = lo.wp(10) + lo.wp(48);
    c.draw_text(&fields::medication(ctx, r, 1), rect(ml, 2 * h2, lo.wp(80), 3 * h2), DT_LEFT);
    c.draw_text(&fields::medication(ctx, r, 2), rect(ml, 3 * h2, lo.wp(80), 4 * h2), DT_LEFT);
    c.draw_text(&fields::exam(ctx, r), rect(lo.wp(48), h2, lo.wp(80), 2 * h2), DT_LEFT);
    c.draw_text(&fields::device_line(r), rect(lo.wp(2), lo.hp(99) - h2, lo.wp(80), lo.hp(99)), DT_LEFT);
    c.draw_text(&fields::date(&r.acquired), rect(lo.wp(80), h2, lo.wp(98), 2 * h2), DT_RIGHT);
    let label = [&b"10mm/mV 25mm/s  "[..], &fields::filter_label(r, p.h_option)].concat();
    let lt = lo.hp(1) + lo.wp(7);
    c.draw_text(&label, rect(lo.wp(2) + lo.wp(5), lt, lo.wp(80), lt + h2), DT_LEFT);
}

/// 0x406e40: calibration pulses of the left column, then each lead's name and 5 s of wave.
fn waves(c: &mut Canvas, lo: &Layout, p: &PageInput, x0: i32, y0: i32) {
    let spp = 20.0 / lo.px;
    let scale = spp * 4.0;
    let cen = &p.centres;
    let old = c.pen();
    c.create_pen(1, BLACK);
    let xs = [x0 + lo.mm(2.0), x0 + lo.mm(3.0), x0 + lo.mm(6.0), x0 + lo.mm(7.0)];
    for &ck in cen.iter().take(6) {
        let y = y0 + ck + lo.mm(5.0);
        cal_pulse(c, xs, y, y - lo.mm(10.0));
    }
    let n = p.filtered.len().min(2500);
    let mut b = 0i32;
    for (k, &ck) in cen.iter().enumerate() {
        let col = (k / 6) as i32;
        if k == 6 {
            b = 0;
        }
        let xoff = lo.mm((125 * col + 10) as f64);
        let l = x0 + xoff + lo.mm(1.0);
        let top = y0 + ck - (ck - b) / 2 - lo.mm(5.0);
        c.draw_text(&lead_name(k, p.japanese), rect(l, top, l + lo.mm(15.0), top + lo.mm(10.0)), DT_LEFT);
        b = 2 * ck - b;
        let data: Vec<i16> = (0..n).map(|j| sample(p.filtered, k, 2500 * col as usize + j)).collect();
        wave::draw(c, &data, x0 + xoff, y0 + ck - p.shifts[k], spp, scale);
    }
    c.select_pen(old.0, old.1);
}
