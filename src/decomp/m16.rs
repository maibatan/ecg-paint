//! Method 16: in-stream Huffman tables, segmented rhythm with average templates, flag columns.

use super::{be16, cdiv, s16, set_record_flags, u16w, Cmp, Machine, Meas};

/// Decode failure inside a section (the reference raises `DecodeError`).
struct DecodeError;

/// Big-endian 16-bit word view starting at a byte offset; reads past the end give 0.
#[derive(Clone, Copy)]
pub(crate) struct Words<'a> {
    b: &'a [u8],
    o: i64,
}

impl<'a> Words<'a> {
    pub fn new(b: Option<&'a [u8]>) -> Self {
        Words { b: b.unwrap_or(&[]), o: 0 }
    }

    pub fn w(&self, i: i64) -> i64 {
        be16(self.b, self.o + 2 * i)
    }

    /// 32 bits starting at bit `bitpos` (MSB first). With `limit`, words >= limit read as 0.
    fn win32(&self, bitpos: i64, limit: Option<i64>) -> u64 {
        let (wi, sh) = (bitpos >> 4, bitpos & 15);
        let g = |i: i64| -> u64 {
            match limit {
                Some(l) if i >= l => 0,
                _ => self.w(i) as u64,
            }
        };
        let mut v = ((g(wi) << 16) | g(wi + 1)) << sh;
        if sh != 0 {
            v |= g(wi + 2) >> (16 - sh);
        }
        v & 0xFFFF_FFFF
    }

    fn sub(&self, start_word: i64) -> Words<'a> {
        Words { b: self.b, o: self.o + 2 * start_word }
    }

    /// Byte at `off` relative to the view start (0 outside the buffer).
    fn byte(&self, off: i64) -> i64 {
        let a = self.o + off;
        if a < 0 {
            0
        } else {
            self.b.get(a as usize).copied().unwrap_or(0) as i64
        }
    }
}

/// Values of the 33 symbols (built on the stack by the DLL) and their extra-bit counts.
pub(crate) const M16_BASE: [i64; 33] = [
    0, -1, 1, -2, 2, -3, 3, -4, 4, -5, 5, -6, 6, -7, 7, -8, 8, -9, 9, -10, 10, -11, 11, -12, 12, -13, 13, -29, 29, -45,
    45, -301, 0,
];
pub(crate) const M16_EXTRA: [u32; 33] =
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 4, 4, 4, 8, 8, 16];

/// Flag bit of channel c (channels 0..7 from the top bit, channel 8 = bit 0).
pub(crate) const FLAG_BIT: [i64; 9] = [0x8000, 0x4000, 0x2000, 0x1000, 0x0800, 0x0400, 0x0200, 0x0100, 0x0001];

/// Mask of the top `n` bits of a 32-bit word.
fn mask32(n: i64) -> u64 {
    if n <= 0 {
        0
    } else {
        (0xFFFF_FFFFu64 << (32 - n.min(32))) & 0xFFFF_FFFF
    }
}

struct CodeTable {
    codes: [u64; 33],
    lens: [i64; 33],
}

/// Code table section at `ws` (word 0 = byte length). Returns the table and the words used.
fn m16_code_table(ws: Words, avail_words: i64) -> Result<(CodeTable, i64), DecodeError> {
    if avail_words < 1 {
        return Err(DecodeError);
    }
    let l = ws.w(0);
    if avail_words < (l >> 1) {
        return Err(DecodeError);
    }
    // the reference reads through a copy of L + 64 bytes
    let raw = |off: i64| if off < l + 64 { ws.byte(off) } else { 0 };
    let mut off = 2i64;
    let mut t = CodeTable { codes: [0; 33], lens: [0; 33] };
    for s in 0..33 {
        if off > l {
            return Err(DecodeError);
        }
        let ln = raw(off);
        off += 1;
        let nb = (ln + 7) >> 3;
        if off + nb > l {
            return Err(DecodeError);
        }
        let mut c = 0u64;
        for k in 0..nb {
            c |= (raw(off) as u64) << ((24 - 8 * k) & 31);
            off += 1;
        }
        t.codes[s] = c & 0xFFFF_FFFF;
        t.lens[s] = ln;
    }
    if ((off + 1) ^ l) & !1 != 0 {
        return Err(DecodeError);
    }
    Ok((t, l >> 1))
}

/// One Huffman segment (BE32 bit count first). Returns (values, words used).
fn m16_segment(ws: Words, maxi: i64, avail_words: i64, t: &CodeTable) -> Result<(Vec<i64>, i64), DecodeError> {
    let nbits = (ws.w(0) << 16) | ws.w(1);
    if avail_words * 16 < nbits || nbits < 32 {
        return Err(DecodeError);
    }
    let used = (nbits + 15) >> 4;
    let mut bitpos = 32i64;
    let mut vals = Vec::new();
    loop {
        let win = ws.win32(bitpos, None);
        let s = (0..33).find(|&s| (win & mask32(t.lens[s])) == t.codes[s]).ok_or(DecodeError)?;
        bitpos += t.lens[s];
        let v = if s > 25 {
            let eb = M16_EXTRA[s];
            let (wi, sh) = (bitpos >> 4, bitpos & 15);
            let w2 = ((((ws.w(wi) << 16) | ws.w(wi + 1)) as u64) << sh) & 0xFFFF_FFFF;
            bitpos += eb as i64;
            s16(M16_BASE[s] + (w2 >> (32 - eb)) as i64)
        } else {
            M16_BASE[s]
        };
        vals.push(v);
        if bitpos >= nbits {
            return Ok((vals, used));
        }
        if vals.len() as i64 > maxi {
            return Err(DecodeError);
        }
    }
}

fn zero_lead_from(m: &mut Machine, count: i64, l: i64, pos: i64) {
    for r in pos.max(0)..count {
        m.rset(16 * r + 4 + l, 0);
    }
}

fn zero_col(m: &mut Machine, count: i64, col: i64) {
    for r in 0..count {
        m.rset(16 * r + col, 0);
    }
}

/// Template window (a, b) from the average window and the fiducial points.
fn m16_window(meas: &Meas) -> (i64, i64, bool) {
    let (x0, x1) = (meas.x0 as i64, meas.x1 as i64);
    if x0 >= x1 || x1 >= 600 {
        return (0, 0, false);
    }
    let f = &meas.f;
    let (p4, p8, pa, pc) = (f.p4 as i64, f.p8 as i64, f.pa as i64, f.pc as i64);
    let a = if x0 < p4 && p4 <= 250 {
        p4
    } else if x0 < p8 && p8 <= 250 {
        p8
    } else if x0 <= 250 {
        x0
    } else {
        0xFFFF
    };
    let b = if 250 <= pc && pc < x1 {
        pc
    } else if 250 <= pa && pa < x1 {
        pa
    } else if (250..600).contains(&x1) {
        x1
    } else {
        0xFFFF
    };
    if a == 0xFFFF || b == 0xFFFF || a > p8 || p8 > pa || pa > b {
        return (0, 0, false);
    }
    (a, b, true)
}

fn zero_avg_leads(m: &mut Machine, from: i64) {
    for l in from..8 {
        for k in 0..600 {
            m.aset(1 + 600 * l + k, 0);
        }
    }
}

/// Average: code table + 8 segments on [x0, x1]; value = 2 x double running sum. Returns 0 / -2.
pub(crate) fn m16_avg(m: &mut Machine, cmp: &Cmp, x0: i64, x1: i64) -> i64 {
    let ws = Words::new(cmp.main);
    let tw = ws.w(0) >> 1;
    if tw > 6000 {
        zero_avg_leads(m, 0);
        return -2;
    }
    let Ok((t, used)) = m16_code_table(ws.sub(1), tw - 1) else { return -2 };
    let mut wp = 1 + used;
    let dstart = wp;
    let dw = ws.w(wp) >> 1;
    if wp + dw > tw {
        return -2;
    }
    wp += 1;
    let n = x1 - x0 + 1;
    let mut temp = vec![[0i64; 600]; 8];
    let mut err = 0;
    for (l, tl) in temp.iter_mut().enumerate() {
        match m16_segment(ws.sub(wp), n, dw - (wp - dstart), &t) {
            Ok((vals, u)) => {
                for (slot, &v) in tl.iter_mut().zip(vals.iter()) {
                    *slot = v;
                }
                wp += u;
            }
            Err(_) => {
                err = -2;
                zero_avg_leads(m, l as i64);
                break;
            }
        }
    }
    for (l, tl) in temp.iter().enumerate() {
        let base = 1 + 600 * l as i64;
        for k in 0..x0 {
            m.aset(base + k, 0);
        }
        for k in x1 + 1..600 {
            m.aset(base + k, 0);
        }
        m.aset(base + x0, tl[0]);
        for j in 1..n {
            let v = if j < 600 { tl[j as usize] } else { 0 };
            m.aset(base + x0 + j, v + m.aget(base + x0 + j - 1));
        }
        for k in x0 + 1..x1 + 1 {
            m.aset(base + k, m.aget(base + k) + m.aget(base + k - 1));
        }
        for k in x0..x1 + 1 {
            m.aset(base + k, m.aget(base + k) << 1);
        }
    }
    if err == 0 && wp - dstart != dw {
        err = -2;
    }
    err
}

/// Gap varint class from the top bits: (bit length, offset, value mask).
fn varint_class(win: u64) -> (i64, i64, u64) {
    if win & 0x8000_0000 == 0 {
        (4, 1, 0x7)
    } else if win & 0x4000_0000 == 0 {
        (8, 9, 0x3F)
    } else if win & 0x2000_0000 == 0 {
        (12, 0x49, 0x1FF)
    } else if win & 0x1000_0000 == 0 {
        (16, 0x249, 0xFFF)
    } else {
        (20, 0x1249, 0xFFFF)
    }
}

/// Toggle lists -> per-record flag word in record column `col`. Returns 0, -1 or -2.
fn m16_flags(m: &mut Machine, ws: Words, nch: usize, count: i64, col: i64, avail: i64) -> i64 {
    let l = ws.w(0) >> 1;
    if avail < l {
        return -2;
    }
    let init = ws.w(1);
    let mut lists: Vec<Vec<i64>> = vec![vec![count]; nch];
    let mut bitpos = 32i64;
    for list in lists.iter_mut() {
        let start = bitpos;
        let wi = bitpos >> 4;
        if l < wi + 1 {
            return -1;
        }
        let seglen = ws.w(wi);
        bitpos += 16;
        let nw = (seglen + 15) >> 4;
        if l < nw + wi {
            return -1;
        }
        if seglen < bitpos - start {
            return -1;
        }
        if seglen == bitpos - start {
            continue;
        }
        let mut lst: Vec<i64> = Vec::new();
        loop {
            let win = ws.win32(bitpos, Some(l));
            let (ln, off, msk) = varint_class(win);
            if seglen < ln + bitpos - start {
                return -1;
            }
            bitpos += ln;
            let v = ((win >> (32 - ln)) & msk) as i64;
            let e = u16w(v + off + lst.last().copied().unwrap_or(0));
            lst.push(e);
            if e > u16w(count) {
                return -1;
            }
            if lst.len() >= 1499 {
                return -1;
            }
            if seglen == bitpos - start {
                lst.push(count);
                if bitpos & 15 != 0 {
                    bitpos = (bitpos & 0xFFF0) + 16;
                }
                break;
            }
        }
        *list = lst;
    }
    if u16w(bitpos) >> 4 != l {
        return -1;
    }
    // merge the channel lists into the flag column
    let mut cnt = vec![0usize; nch];
    // (every list ends with `count`, so an index past the end cannot be reached by valid control
    // flow; it is treated as the end of the merge instead of a crash)
    let at = |lists: &Vec<Vec<i64>>, cnt: &Vec<usize>, c: usize| lists[c].get(cnt[c]).copied();
    m.rset(col, s16(init));
    let mut state = init;
    let mut cur = 0usize;
    for c in 1..nch {
        if lists[c][0] < lists[cur][0] {
            cur = c;
        }
    }
    for c in 0..nch {
        if c != cur && lists[c][0] == lists[cur][0] {
            state ^= FLAG_BIT[c];
            cnt[c] += 1;
        }
    }
    let mut prev = init;
    let mut pos = 1i64;
    state ^= FLAG_BIT[cur];
    loop {
        let Some(p) = at(&lists, &cnt, cur) else { return 0 };
        cnt[cur] += 1;
        if pos < p {
            for r in pos..p {
                if r < m.cap as i64 {
                    m.rset(16 * r + col, s16(prev));
                }
            }
            pos = p;
        }
        if pos >= count {
            return 0;
        }
        prev = state;
        let mut nxt = 0usize;
        for c in 1..nch {
            let (Some(vc), Some(vn)) = (at(&lists, &cnt, c), at(&lists, &cnt, nxt)) else { return 0 };
            if vc < vn {
                nxt = c;
            }
        }
        let Some(vn) = at(&lists, &cnt, nxt) else { return 0 };
        for c in 0..nch {
            if c == nxt {
                continue;
            }
            let Some(vc) = at(&lists, &cnt, c) else { return 0 };
            if vc == vn {
                cnt[c] += 1;
                state ^= FLAG_BIT[c];
            }
        }
        state ^= FLAG_BIT[nxt];
        cur = nxt;
    }
}

/// 9-channel flags at the end of the main block -> column 1. Returns 0, -4 or -6.
fn m16_flags_main(m: &mut Machine, cmp: &Cmp, count: i64) -> i64 {
    let ws = Words::new(cmp.main);
    let w0 = ws.w(0) & 0xFFFE;
    let err;
    if w0 > 12000 {
        err = -4;
    } else {
        let d = (ws.w(1) >> 1) + 1;
        if d > 6000 {
            err = -4;
        } else {
            let d2 = d + (ws.w(d) >> 1);
            let tw = ws.w(0) >> 1;
            if d2 >= 6000 || tw <= d2 {
                err = -4;
            } else {
                match m16_flags(m, ws.sub(d2), 9, count, 1, tw - d2) {
                    -1 => err = -4,
                    -2 => err = -6,
                    _ => return if d2 + (ws.w(d2) >> 1) == tw { 0 } else { -6 },
                }
            }
        }
    }
    zero_col(m, count, 1);
    err
}

/// 8-channel flags at the end of lead blocks 0 and 1 -> columns 2 and 3. Returns -(error bits).
fn m16_flags_leads(m: &mut Machine, cmp: &Cmp, count: i64) -> i64 {
    let mut err = 0i64;
    for idx in 0..2usize {
        let col = 2 + idx as i64;
        let code = if idx == 0 { 8 } else { 0x10 };
        let ws = Words::new(cmp.leads[idx]);
        let tw = ws.w(0) >> 1;
        let mut bad = tw > count;
        let mut e = 1i64;
        if !bad && idx == 0 {
            let nb = ws.w(1);
            e = (nb >> 4) + if nb % 16 == 0 { 2 } else { 3 };
        }
        if !bad && e >= count {
            bad = true;
        }
        if !bad {
            e += ws.w(e) >> 1;
            bad = e >= count;
        }
        if !bad {
            e += ws.w(e) >> 1;
            bad = e >= count;
        }
        if !bad && tw <= e {
            bad = true;
        }
        if bad {
            zero_col(m, count, col);
            err |= code;
            continue;
        }
        match m16_flags(m, ws.sub(e), 8, count, col, tw - e) {
            -1 => {
                zero_col(m, count, col);
                err |= code;
            }
            -2 => {
                zero_col(m, count, col);
                err |= code | 1;
            }
            _ => {
                if e + (ws.w(e) >> 1) != tw {
                    err |= code | 1;
                }
            }
        }
    }
    -err
}

/// Type 0: full-rate values. Returns the new position or -1.
fn place_full(m: &mut Machine, count: i64, vals: &[i64], l: i64, pos: i64) -> i64 {
    let n = vals.len() as i64;
    if pos + n > count {
        return -1;
    }
    for (i, &v) in vals.iter().enumerate() {
        m.rset(16 * (pos + i as i64) + 4 + l, s16(v));
    }
    pos + n
}

/// Type 1: decimated values with midpoints, the last 4 at full rate.
fn place_half(m: &mut Machine, count: i64, vals: &[i64], n: i64, l: i64, pos: i64, param: i64) -> i64 {
    let col = 4 + l;
    let v = |i: i64| if i >= 0 { vals.get(i as usize).copied().unwrap_or(0) } else { 0 };
    if n <= 4 {
        if pos + n > count {
            return -1;
        }
        for i in 0..n {
            m.rset(16 * (pos + i) + col, v(i));
        }
        return pos + n;
    }
    let mm = n - 4;
    if pos + param + 2 * mm + 3 > count {
        return -1;
    }
    m.rset(16 * pos + col, v(0));
    let mut p = pos + 2;
    let mut i = 1;
    while i < mm {
        m.rset(16 * p + col, v(i));
        let mid = cdiv(m.rget(16 * (p - 2) + col) + m.rget(16 * p + col), 2);
        m.rset(16 * (p - 1) + col, mid);
        p += 2;
        i += 1;
    }
    if param == 0 {
        p = u16w(p - 1);
    }
    m.rset(16 * p + col, v(i));
    p += 1;
    i += 1;
    m.rset(16 * p + col, v(i));
    p += 1;
    i += 1;
    m.rset(16 * p + col, v(i));
    p += 1;
    m.rset(16 * p + col, v(i + 1));
    p += 1;
    if param == 1 {
        let mid = cdiv(m.rget(16 * (p - 6) + col) + m.rget(16 * (p - 4) + col), 2);
        m.rset(16 * (p - 5) + col, mid);
    }
    p
}

/// Types 2/3: placement around the average template `tt` (600 entries).
#[allow(clippy::too_many_arguments)]
fn place_beat(
    m: &mut Machine,
    count: i64,
    vals: &mut [i64],
    n: i64,
    tt: &[i64; 600],
    meas: &Meas,
    mut pos: i64,
    l: i64,
    k0: i64,
    param: i64,
) -> i64 {
    let tk = |k: i64| if (0..600).contains(&k) { tt[k as usize] } else { 0 };
    // the reference would raise IndexError past the segment end; treat it as a placement error
    let nv = vals.len() as i64;
    let (on, off) = (meas.f.p8 as i64, meas.f.pa as i64);
    let mut i = 0i64;
    if k0 < on {
        let par8 = (on - k0 - 1) & 1;
        let lim = s16(on - 4);
        if cdiv(on - k0 - 3, 2) > n {
            return -1;
        }
        let mut k = k0;
        while s16(k) < on - 4 {
            if i >= nv {
                return -1;
            }
            vals[i as usize] = s16(vals[i as usize] + tk(k));
            k += 2;
            i += 1;
        }
        if on - lim > n - i {
            return -1;
        }
        k = lim;
        while k < on {
            if i >= nv {
                return -1;
            }
            vals[i as usize] = s16(vals[i as usize] + tk(k));
            i += 1;
            k += 1;
        }
        pos = place_half(m, count, vals, i, l, pos, par8);
        if pos == -1 {
            return -1;
        }
    }
    let end_k = if (n - i) <= (off - on) { s16(on - i + n) } else { s16(off + 1) };
    if pos - on + end_k > count {
        return -1;
    }
    let mut k = on;
    while s16(k) < end_k {
        if i >= nv || i < 0 {
            return -1;
        }
        m.rset(16 * pos + 4 + l, vals[i as usize] + tk(k));
        pos += 1;
        i += 1;
        k += 1;
    }
    let tail = i;
    while i < n - 4 {
        if i >= nv || i < 0 {
            return -1;
        }
        vals[i as usize] = s16(vals[i as usize] + tk(k));
        k += 2;
        i += 1;
    }
    if param == 0 {
        k -= 1;
    }
    while i < n {
        if i >= nv || i < 0 {
            return -1;
        }
        vals[i as usize] = s16(vals[i as usize] + tk(k));
        i += 1;
        k += 1;
    }
    let tail_u = (tail.max(0) as usize).min(vals.len());
    place_half(m, count, &vals[tail_u..], n - tail, l, pos, param)
}

/// Rhythm leads. Returns 0 or -1 (the pending error of lead 7 is dropped, like the original).
fn m16_rhythm(m: &mut Machine, cmp: &Cmp, meas: &Meas, count: i64, avg_err: i64, a: i64, b: i64) -> i64 {
    let mut result = 0i64;
    let mut pending = 0i64;
    let mut tt = [0i64; 600];
    let mut types: Vec<i64> = Vec::new();
    let mut params: Vec<i64> = Vec::new();
    let mut nseg = 0i64;
    for l in 0..8i64 {
        result |= pending;
        pending = 0;
        if a <= b {
            for k in a..=b {
                if (0..600).contains(&k) {
                    tt[k as usize] = s16(m.aget(1 + 600 * l + k) << 1);
                }
            }
        }
        let ws = Words::new(cmp.leads[l as usize]);
        let tw = ws.w(0) >> 1;
        if tw > cmp.count as i64 {
            pending = -1;
            zero_lead_from(m, count, l, 0);
            continue;
        }
        let mut wp = 1i64;
        if l == 0 {
            let nb = ws.w(1);
            if nb == 0 || nb > 0x1800 {
                for ll in 0..8 {
                    zero_lead_from(m, count, ll, 0);
                }
                return -1;
            }
            let nwd = (nb >> 4) + if nb & 15 != 0 { 1 } else { 0 } + 2;
            wp = nwd;
            nseg = nb >> 2;
            types.clear();
            params.clear();
            for s in 0..nseg {
                let bp = 4 * s;
                let word = ws.w(2 + (bp >> 4));
                let sh = bp & 15;
                types.push((word >> (14 - sh)) & 3);
                params.push((word >> (12 - sh)) & 3);
            }
        }
        let Ok((t, used)) = m16_code_table(ws.sub(wp), tw - wp) else {
            pending = -1;
            zero_lead_from(m, count, l, 0);
            continue;
        };
        wp += used;
        let dstart = wp;
        let dw = ws.w(wp) >> 1;
        if wp + dw > tw {
            pending = -1;
            zero_lead_from(m, count, l, 0);
            continue;
        }
        wp += 1;
        let mut v: Vec<i64> = Vec::new();
        let mut seglen: Vec<usize> = Vec::new();
        let mut seg_err = 0i64;
        for _ in 0..nseg {
            let rem = u16w(count - v.len() as i64).min(4000);
            match m16_segment(ws.sub(wp), rem, dw - wp + dstart, &t) {
                Ok((vals, u)) => {
                    seglen.push(vals.len());
                    v.extend(vals);
                    wp += u;
                }
                Err(_) => {
                    seg_err = -1;
                    break;
                }
            }
        }
        for j in 1..v.len() {
            v[j] = s16(v[j] + v[j - 1]);
        }
        let mut pos = 0i64;
        let mut off = 0usize;
        for (s, &n) in seglen.iter().enumerate() {
            let typ = types.get(s).copied().unwrap_or(0);
            let param = params.get(s).copied().unwrap_or(0);
            if typ > 1 && avg_err != 0 {
                for ll in 0..8 {
                    zero_lead_from(m, count, ll, pos);
                }
                return -1;
            }
            for x in v[off..off + n].iter_mut() {
                *x = s16(*x << 3);
            }
            let mut seg: Vec<i64> = v[off..off + n].to_vec();
            pos = match typ {
                0 => place_full(m, count, &seg, l, pos),
                1 => place_half(m, count, &seg, n as i64, l, pos, param),
                _ => {
                    let k0 = if typ == 2 { a } else { meas.f.p8 as i64 };
                    place_beat(m, count, &mut seg, n as i64, &tt, meas, pos, l, k0, param)
                }
            };
            if pos == -1 {
                pos = 0xFFFF;
                seg_err = -1;
                break;
            }
            off += n;
        }
        if seg_err != 0 {
            pending = seg_err;
            zero_lead_from(m, count, l, pos);
        } else if wp - dstart != dw {
            result |= -1;
        }
    }
    result
}

pub(super) fn m16(m: &mut Machine, cmp: &Cmp, meas: &Meas) -> i64 {
    let count = cmp.count as i64;
    if count > m.cap as i64 {
        return -64;
    }
    m.avg[0] = 0x3842;
    let (a, b, ok) = m16_window(meas);
    let e_win = if ok { 0 } else { -2 };
    let (x0, x1) = (meas.x0 as i64, meas.x1 as i64);
    let e_avg = if x0 < x1 && x1 < 600 {
        m16_avg(m, cmp, x0, x1)
    } else if Words::new(cmp.main).w(1) == 2 {
        0
    } else {
        -2
    };
    let mut e_flags = m16_flags_main(m, cmp, count);
    let e_rh = if (1..=12000).contains(&count) { m16_rhythm(m, cmp, meas, count, e_win + e_avg, a, b) } else { -1 };
    e_flags += m16_flags_leads(m, cmp, count);
    set_record_flags(m, count as usize, meas);
    s16(-((-e_rh) | (-e_flags) | (-e_avg)))
}
