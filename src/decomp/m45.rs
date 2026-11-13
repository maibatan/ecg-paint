//! Methods 4 and 5: unary/escape codes, 2nd-order average, half-rate rhythm + template.

use super::{be16, cdiv, qrs_entries, s16, set_record_flags, u16w, Cmp, Machine, Meas};

/// Byte pointer + shift register, MSB first. The pointer starts on the 2nd byte of the BE16
/// length and the block ends at `1 + len - 1` (signed length, like the DLL).
pub(crate) struct M45Reader<'a> {
    b: &'a [u8],
    ptr: i64,
    end: i64,
    rem: u32,
    cur: u32,
}

impl<'a> M45Reader<'a> {
    pub fn new(blk: Option<&'a [u8]>) -> Self {
        let b = blk.unwrap_or(&[]);
        let ln = s16(be16(b, 0));
        M45Reader { b, ptr: 1, end: ln, rem: 0, cur: 0 }
    }

    pub fn more(&self) -> bool {
        self.ptr < self.end
    }

    pub fn bit(&mut self) -> i64 {
        if self.rem == 0 {
            self.ptr += 1;
            self.cur = if self.ptr >= 0 { self.b.get(self.ptr as usize).copied().unwrap_or(0) as u32 } else { 0 };
            self.rem = 8;
        } else {
            self.cur = (self.cur << 1) & 0xFF;
        }
        self.rem -= 1;
        ((self.cur >> 7) & 1) as i64
    }

    pub fn bits(&mut self, n: u32) -> i64 {
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | self.bit();
        }
        v
    }
}

/// Escape value: 4 bits a != 0 -> a - 8; else 8 bits t -> t - 8 (t - 263 when that has bit 7
/// set); else 16 bits as int16.
pub(crate) fn m45_escape(r: &mut M45Reader) -> i64 {
    let a = r.bits(4);
    if a != 0 {
        return a - 8;
    }
    let b = r.bits(4);
    if b != 0 {
        let mut t = ((b << 4) | r.bits(4)) - 8;
        if t & 0x80 != 0 {
            t -= 255;
        }
        return t;
    }
    s16(r.bits(16))
}

/// Count leading 1 bits (the terminating 0 is consumed), at most `kmax`.
fn m45_unary(r: &mut M45Reader, kmax: i64) -> i64 {
    let mut k = 0;
    while r.bit() != 0 {
        k += 1;
        if k >= kmax {
            break;
        }
    }
    k
}

/// Average block of methods 4/5: 8 leads x `nper` values stored as 4d, then per lead
/// x[j] += 2 x[j-1] - x[j-2] (j >= 2).
pub(crate) fn m45_avg(m: &mut Machine, blk: Option<&[u8]>, nper: i64) {
    let mut r = M45Reader::new(blk);
    let n = 8 * nper;
    let mut i = 0;
    while i < n && r.more() {
        let k = m45_unary(&mut r, 7);
        let d = if k == 0 {
            0
        } else if k < 7 {
            if r.bit() != 0 {
                -k
            } else {
                k
            }
        } else {
            m45_escape(&mut r)
        };
        m.aset(1 + i, s16(d << 2));
        i += 1;
    }
    for l in 0..8 {
        let base = 1 + l * nper;
        for j in 2..nper {
            let v = m.aget(base + j) + 2 * m.aget(base + j - 1) - m.aget(base + j - 2);
            m.aset(base + j, s16(v));
        }
    }
}

/// Lead L at half rate into 13-word rows, integrate, upsample x2 linearly in place.
/// `zero_runs` = method 4 ('0' + escape = run of zeros, buggy 14-word fill step);
/// otherwise method 5 ('0' = one zero, 13-word fill step).
fn m45_rhythm_lead(m: &mut Machine, blk: Option<&[u8]>, l: i64, count: i64, zero_runs: bool) {
    let m_half = u16w(count) >> 1;
    let col = 1 + l;
    let end_w = 13 * m_half + 1; // first word past the half-rate rows
    let mut p = col;
    let mut r = M45Reader::new(blk);
    let mut stop = false;
    while p < end_w && !stop {
        if !r.more() {
            break;
        }
        let k = m45_unary(&mut r, 4);
        let d = if k == 0 {
            if zero_runs {
                let mut run = s16(m45_escape(&mut r) + 8);
                while p < end_w && run > 0 {
                    m.rset(p, 0);
                    run -= 1;
                    p += 13;
                }
                if run != 0 {
                    stop = true;
                }
                continue;
            }
            0
        } else if k < 4 {
            if r.bit() != 0 {
                -k
            } else {
                k
            }
        } else {
            m45_escape(&mut r)
        };
        m.rset(p, d << 2);
        p += 13;
    }
    let step = if zero_runs { 14 } else { 13 };
    while p < end_w {
        m.rset(p, 0);
        p += step;
    }
    // first-order integration at half rate
    for k in 1..m_half {
        let v = m.rget(13 * k + col) + m.rget(13 * (k - 1) + col);
        m.rset(13 * k + col, v);
    }
    // linear x2 upsampling, walking backwards in place
    if m_half >= 1 {
        let mut src = m_half - 1;
        let mut dst = count - 1;
        m.rset(13 * dst + col, m.rget(13 * src + col));
        dst -= 2;
        while src >= 1 {
            let a = m.rget(13 * src + col);
            let b = m.rget(13 * (src - 1) + col);
            m.rset(13 * (dst + 1) + col, (3 * a + b) >> 2);
            let a2 = m.rget(13 * (src - 1) + col);
            let b2 = m.rget(13 * src + col);
            m.rset(13 * dst + col, (3 * a2 + b2) >> 2);
            src -= 1;
            dst -= 2;
        }
    }
}

/// Add/subtract a quarter of the average at a template edge (keeps the integrated signal level).
fn edge(m: &mut Machine, row: i64, col: i64, a: i64) {
    m.rset(13 * (row - 1) + col, m.rget(13 * (row - 1) + col) + a);
    m.rset(13 * row + col, m.rget(13 * row + col) - a);
}

/// Method 4: add the average beat around every normal QRS (full rate, fiducial 250 of 600).
fn m4_template(m: &mut Machine, meas: &Meas, l: i64, count: i64) {
    let col = 1 + l;
    let ent = qrs_entries(meas);
    let mut prev_end = 0i64;
    for (idx, &(pos_u, b2)) in ent.iter().enumerate() {
        let pos = s16(pos_u as i64);
        let b2 = b2 as i64;
        let typ = b2 & 7;
        if typ != 0 && typ != 5 {
            prev_end = s16(pos + 60);
            continue;
        }
        let mut start = if prev_end >= pos - 249 { prev_end } else { pos - 249 };
        start = s16((start + 1) & !1);
        let nxt = match ent.get(idx + 1) {
            Some(e) => e.0 as i64,
            None => u16w(count + 120),
        };
        let mut t = s16(nxt - 469 - pos);
        if t > 0 {
            t = 0;
        }
        let end = s16((pos + t + 349) & !1);
        prev_end = end;
        let mut k = 1 + 250 + 600 * l + (start - pos);
        if start > 0 {
            let a = cdiv(m.aget(k), 4);
            edge(m, start, col, a);
        }
        let mode = (b2 >> 3) & 7;
        if !(1..=3).contains(&mode) {
            continue;
        }
        let mut rr = start;
        while 13 * rr + col < 13 * end + 1 {
            let add = match mode {
                1 => (m.aget(k - 1) + m.aget(k)) >> 1,
                2 => m.aget(k),
                _ => (m.aget(k) + m.aget(k + 1)) >> 1,
            };
            m.rset(13 * rr + col, m.rget(13 * rr + col) + add);
            rr += 1;
            k += 1;
        }
        let a = cdiv(m.aget(k - 1), 4);
        edge(m, rr, col, -a);
    }
}

/// Method 5: the same at half the final rate, fiducial 125 of a 300-sample average.
fn m5_template(m: &mut Machine, meas: &Meas, l: i64, count: i64) {
    let col = 1 + l;
    let ent = qrs_entries(meas);
    let mut prev_end = 0i64;
    for (idx, &(pos_u, b2)) in ent.iter().enumerate() {
        let pos = s16(pos_u as i64);
        let b2 = b2 as i64;
        let typ = b2 & 7;
        if typ != 0 && typ != 5 {
            prev_end = s16(pos + 30);
            continue;
        }
        let mut start = if prev_end >= pos - 124 { prev_end } else { pos - 124 };
        start = s16((start + 1) & !1);
        let nxt = match ent.get(idx + 1) {
            Some(e) => e.0 as i64,
            None => u16w(count + 60),
        };
        let mut di = s16(nxt - pos - 234);
        if di > 0 {
            di = 0;
        }
        di = s16(di + 174);
        if di < 30 {
            di = 30;
        }
        let end = s16((di + pos) & !1);
        let mut k = 1 + 125 + 300 * l + (start - pos);
        if start > 0 {
            let a = cdiv(m.aget(k), 4);
            edge(m, start, col, a);
        }
        let mode = (b2 >> 3) & 7;
        let mut rr = start;
        while 13 * rr + col < 13 * end + 1 {
            let add = match mode {
                0 | 4 => (m.aget(k) + m.aget(k + 1)) >> 1,
                1 => (3 * m.aget(k) + m.aget(k - 1)) >> 2,
                3 => (3 * m.aget(k) + m.aget(k + 1)) >> 2,
                _ => m.aget(k),
            };
            m.rset(13 * rr + col, m.rget(13 * rr + col) + add);
            rr += 1;
            k += 1;
        }
        let a = cdiv(m.aget(k - 1), 4);
        edge(m, rr, col, -a);
        prev_end = end;
    }
}

/// 26-byte work rows -> 32-byte records, backwards in place; words 1..3 cleared.
fn rows_to_records(m: &mut Machine, n: i64) {
    for r in (0..n).rev() {
        let mut w = [0i64; 9];
        for (c, slot) in w.iter_mut().enumerate() {
            *slot = m.rget(13 * r + c as i64);
        }
        for c in 12..16 {
            m.rset(16 * r + c, 0);
        }
        for l in 0..8 {
            m.rset(16 * r + 4 + l, w[1 + l as usize]);
        }
        m.rset(16 * r, w[0]);
    }
    for r in 0..n {
        m.rset(16 * r + 1, 0);
        m.rset(16 * r + 2, 0);
        m.rset(16 * r + 3, 0);
    }
}

/// Catmull-Rom x2 at quarter offsets. `d` = 128 (gain 1) or 64 (gain 2). Fewer than 2 inputs are
/// padded with zeros (the output then has 4 values).
pub(crate) fn catmull_up2(x: &[i64], d: i64) -> Vec<i64> {
    let mut x = x.to_vec();
    if x.len() < 2 {
        x.resize(2, 0);
    }
    let n = x.len();
    let mut out = Vec::with_capacity(2 * n);
    out.push(cdiv(137 * x[0] - 9 * x[1], d));
    let (mut p, mut a, mut c) = (x[0], x[0], x[1]);
    for j in 0..n - 2 {
        let dn = x[j + 2];
        out.push(cdiv(-9 * p + 111 * a + 29 * c - 3 * dn, d));
        out.push(cdiv(-3 * p + 29 * a + 111 * c - 9 * dn, d));
        p = a;
        a = c;
        c = dn;
    }
    out.push(cdiv(111 * a + 26 * c - 9 * p, d));
    out.push(cdiv(29 * a + 102 * c - 3 * p, d));
    out.push(cdiv(137 * c - 9 * a, d));
    out.into_iter().map(s16).collect()
}

/// Method 5: every time position of the measurement data x 2 (QRS list cut to 256 entries,
/// list 2 to 512, like the original).
pub(crate) fn m5_double_meas(meas: &mut Meas) {
    meas.x0 = (meas.x0 as u32 * 2) as u16;
    meas.x1 = (meas.x1 as u32 * 2) as u16;
    if meas.f.w0 != 0xFFFF {
        meas.f.w2 = (meas.f.w2 as u32 * 2) as u16;
    }
    let mut new = Vec::new();
    for &(pos, b2) in meas.qrs.iter().take(256) {
        if pos == 0xFFFF {
            new.push((pos, b2));
            break;
        }
        let ph = (b2 >> 3) & 0xF;
        let pos2 = (2 * pos as u32 + (ph >= 2) as u32) as u16;
        let nm: u8 = match ph {
            0 => 1,
            1 => 2,
            2 => 3,
            3 => 2,
            4 => 3,
            _ => 2,
        };
        new.push((pos2, (b2 & 0xC7) | (nm << 3)));
    }
    meas.qrs = new;
    meas.list2 = meas.list2.iter().take(512).map(|&p| if p == 0xFFFF { p } else { (p as u32 * 2) as u16 }).collect();
}

pub(super) fn m4(m: &mut Machine, cmp: &Cmp, meas: &Meas) -> i64 {
    let count = cmp.count as i64;
    if (m.cap as i64) < count {
        return -1;
    }
    m45_avg(m, cmp.main, 600);
    for l in 0..8 {
        m45_rhythm_lead(m, cmp.leads[l as usize], l, count, true);
        m4_template(m, meas, l, count);
        for r in 1..count {
            let w = 13 * r + 1 + l;
            m.rset(w, m.rget(w) << 1);
        }
    }
    m.avg[0] = 0x3842;
    rows_to_records(m, count);
    set_record_flags(m, count as usize, meas);
    0
}

/// Method 5 rewrites the average window before decoding.
pub(crate) fn m5_pre(meas: &mut Meas) {
    meas.x0 = 125u16.wrapping_sub(meas.x0);
    meas.x1 = meas.x1.wrapping_add(if meas.x1 != 0 { 124 } else { 125 });
}

/// Average 300 -> 600 per lead (Catmull-Rom, gain 1).
pub(crate) fn m5_avg_up(m: &mut Machine) {
    let old: Vec<i64> = (0..4800).map(|i| m.aget(1 + i)).collect();
    for l in 0..8usize {
        let y = catmull_up2(&old[300 * l..300 * l + 300], 128);
        for (k, &v) in y.iter().enumerate() {
            m.aset(1 + 600 * l as i64 + k as i64, v);
        }
    }
}

/// Method 5. Ok(record count = 2 x count, wrapped to 16 bits) or Err(-1) when the capacity is short.
pub(super) fn m5(m: &mut Machine, cmp: &Cmp, meas: &mut Meas) -> Result<usize, i64> {
    let count = cmp.count as i64;
    if (m.cap as i64) < 2 * count {
        return Err(-1);
    }
    m5_pre(meas);
    m45_avg(m, cmp.main, 300);
    for l in 0..8 {
        m45_rhythm_lead(m, cmp.leads[l as usize], l, count, false);
        m5_template(m, meas, l, count);
    }
    m5_double_meas(meas);
    // rows -> copy -> Catmull-Rom (gain 2) -> 2 x count rows
    let n = count;
    for r in 0..n {
        for c in 0..9 {
            m.rset(13 * (n + r) + c, m.rget(13 * r + c));
        }
        m.rset(13 * (n + r), 0);
        m.rset(13 * r, 0);
    }
    for l in 0..8 {
        let col = 1 + l;
        let x: Vec<i64> = (0..n).map(|r| m.rget(13 * (n + r) + col)).collect();
        let y = catmull_up2(&x, 64);
        for (r, &v) in y.iter().enumerate() {
            m.rset(13 * r as i64 + col, v);
        }
    }
    let n2 = u16w(2 * n);
    for &(pos, b2) in meas.qrs.iter().take(256) {
        if pos == 0xFFFF {
            break;
        }
        let w = 13 * pos as i64;
        m.rset(w, (m.rget(w) & 0xFF00) | (((b2 as i64 & 3) + 1) & 0xFF));
    }
    for &pos in meas.list2.iter().take(512) {
        if pos == 0xFFFF {
            break;
        }
        let w = 13 * pos as i64;
        m.rset(w, (m.rget(w) & 0x00FF) | 0x8000);
    }
    m5_avg_up(m);
    m.avg[0] = 0x3842;
    rows_to_records(m, n2);
    set_record_flags(m, n2 as usize, meas);
    Ok(n2 as usize)
}
