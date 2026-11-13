//! Wave drawing (0x405650 with 0x405570 / 0x405490) and the per-lead baseline values the
//! wave object computes at load time (0x405db0 → wave+0x1a0, 0x406010 → wave+0x1d0).
//! Every column of pixels is one vertical MoveToEx/LineTo run from the column's max to its
//! min; each column starts from the previous one's range so the trace stays connected.

use super::{t, Canvas};
use crate::model::DISP2STORE;

/// Draw `samples` from column `x`, baseline `y`; `spp` samples per pixel (double), `scale`
/// LSB per pixel. The first column only primes the min/max and is not drawn; a trailing
/// partial column is dropped.
pub fn draw(c: &mut Canvas, samples: &[i16], mut x: i32, y: i32, spp: f64, scale: f64) {
    let k0 = t(spp);
    let st = t(scale);
    let (mut mx, mut mn): (i16, i16) = (i16::MIN, i16::MAX);
    let (mut first, mut j, mut k) = (true, 0i32, k0);
    let (mut acc_d, mut acc_i) = (0.0f64, 0i64);
    for &v in samples {
        if j == 0 {
            let old = mx;
            mx = (mn as i32 - st) as i16;
            mn = (old as i32 + st) as i16;
        }
        if v < mn {
            mn = v;
        }
        if v > mx {
            mx = v;
        }
        j += 1;
        if j >= k {
            j = 0;
            if first {
                first = false;
            } else {
                let a = t(mx as f64 / scale) + 1;
                let mut b = t(mn as f64 / scale);
                if a == b {
                    b += 1;
                }
                c.segment(x, y - a, y - b);
                x += 1;
            }
            acc_d += spp;
            acc_i += k as i64;
            k = if acc_d - acc_i as f64 >= 1.0 { k0 + 1 } else { k0 };
        }
    }
}

/// Value of display lead `disp` at sample `i` of the filtered records (0 past the end).
pub fn sample(recs: &[[i16; 16]], disp: usize, i: usize) -> i16 {
    recs.get(i).map(|r| r[4 + DISP2STORE[disp]]).unwrap_or(0)
}

/// (min, max) used by both baseline functions: sample 1, plus samples e+1 .. 2e−1 with
/// e = (N/2)·(disp/6) — limb leads therefore only look at sample 1.
fn min_max(recs: &[[i16; 16]], disp: usize) -> (i16, i16) {
    let n = recs.len();
    let e = (n / 2) * (disp / 6);
    let mut mn = sample(recs, disp, 1);
    let mut mx = mn;
    if e > 1 {
        for i in e + 1..2 * e {
            let v = sample(recs, disp, i);
            mn = mn.min(v);
            mx = mx.max(v);
        }
    }
    (mn, mx)
}

/// C signed division (truncates toward zero).
fn cdiv(a: i32, b: i32) -> i32 {
    if b == 0 {
        0
    } else {
        a.wrapping_div(b)
    }
}

/// 0x405db0(trunc(40·px), px) → wave+0x1a0: vertical shift per display lead.
pub fn shifts(recs: &[[i16; 16]], px: f64) -> [i32; 12] {
    let f40 = t(px * 40.0);
    let lim = (6 * f40) >> 2;
    let s80 = 80.0 / px;
    let mut out = [0i32; 12];
    for (disp, o) in out.iter_mut().enumerate() {
        let (mn, mx) = min_max(recs, disp);
        if mn == i16::MIN || mx == i16::MIN {
            *o = 0;
            continue;
        }
        let sum = t(mn as f64 / s80) + t(mx as f64 / s80);
        *o = if sum >= lim {
            -(lim >> 1)
        } else if sum < -lim {
            lim >> 1
        } else {
            -(sum >> 1)
        };
    }
    out
}

/// 0x406010(trunc(170·px), px) → wave+0x1d0: row centre per display lead (6 rows per column,
/// rows sized by each lead's range, the left-over height spread between them).
pub fn centres(recs: &[[i16; 16]], px: f64) -> [i32; 12] {
    let f = t(px * 170.0);
    let lo = cdiv(cdiv(f, 2), 6);
    let hi = cdiv(2 * f, 6);
    let s80 = 80.0 / px;
    let mut h = [0i32; 12];
    for (disp, hv) in h.iter_mut().enumerate() {
        let (mn, mx) = min_max(recs, disp);
        *hv = if mn == i16::MIN || mx == i16::MIN {
            hi / 2
        } else {
            t((mx as i32 - mn as i32) as f64 / s80).clamp(lo, hi.max(lo))
        };
    }
    let rem = [f - h[0..6].iter().sum::<i32>(), f - h[6..12].iter().sum::<i32>()];
    let mut c = [0i32; 12];
    for k in (0..12).rev() {
        let r = k % 6;
        let s: i32 = (1..=r).map(|j| h[k - j]).sum::<i32>() + cdiv(h[k], 2);
        let rm = rem[k / 6];
        c[k] = if rm > 0 { s + cdiv(rm * (2 * r as i32 + 1), 12) } else { cdiv(s * f, f - rm) };
    }
    c
}
