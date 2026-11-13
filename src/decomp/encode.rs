//! Encoders for the compressed formats (test-input generators), ported from
//! `research/py/decomp_encode.py`. Method 5 has no Python counterpart in decomp_encode.py; its
//! encoder here mirrors `tests/gen_golden.py::encode_m5` (same rules as method 4 with the
//! method-5 lead codes).
//!
//! The encoders never panic; impossible requests (a plan that does not cover `count`, too many
//! flag toggles, ...) return `Err` from the `encode_*` functions. A block longer than 0xFFFF
//! bytes gets its length field wrapped to 16 bits (such a block cannot be represented).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::m16::{FLAG_BIT, M16_BASE, M16_EXTRA};
use super::{cdiv, s16, Cmp, DEFAULT_TABLE};

// ----------------------------------------------------------------------------- bit writer

/// MSB-first bit writer.
#[derive(Default)]
pub struct BitWriter {
    out: Vec<u8>,
    acc: u32,
    n: u32,
    /// Total bits written.
    pub total: u64,
}

impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Write the low `nbits` bits of `value` (two's complement for negative values).
    pub fn put(&mut self, value: i64, nbits: u32) {
        for i in (0..nbits).rev() {
            let bit = if i < 64 { ((value >> i) & 1) as u32 } else { (value < 0) as u32 };
            self.acc = (self.acc << 1) | bit;
            self.n += 1;
            if self.n == 8 {
                self.out.push(self.acc as u8);
                self.acc = 0;
                self.n = 0;
            }
        }
        self.total += nbits as u64;
    }

    /// Write a string of '0'/'1' characters.
    pub fn put_str(&mut self, bits: &str) {
        for ch in bits.bytes() {
            self.put((ch == b'1') as i64, 1);
        }
    }

    /// Flush (zero padding) to a multiple of `align` bytes.
    pub fn bytes(&self, align: usize) -> Vec<u8> {
        let mut out = self.out.clone();
        if self.n != 0 {
            out.push(((self.acc << (8 - self.n)) & 0xFF) as u8);
        }
        while align > 1 && out.len() % align != 0 {
            out.push(0);
        }
        out
    }
}

fn be16b(v: i64) -> [u8; 2] {
    let v = v as u16;
    v.to_be_bytes()
}

fn be32b(v: i64) -> [u8; 4] {
    (v as u32).to_be_bytes()
}

/// Prepend the BE16 block length (counts its own 2 bytes).
pub fn with_len(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 2);
    out.extend_from_slice(&be16b(payload.len() as i64 + 2));
    out.extend_from_slice(payload);
    out
}

// ----------------------------------------------------------------------------- methods 0 / 1

/// Plain BE16 values (method 0 payload / DecompressWave unpack source).
pub fn pack_be16(values: &[i16]) -> Vec<u8> {
    values.iter().flat_map(|&v| v.to_be_bytes()).collect()
}

/// First default-table entry able to carry difference `d`: (code, extra bit count, extra bits).
fn huff_pick(d: i64) -> (&'static str, u32, i64) {
    for &(code, base, nb) in DEFAULT_TABLE.iter() {
        let (base, nb) = (base as i64, nb as u32);
        if nb == 0 {
            if d == base {
                return (code, 0, 0);
            }
        } else {
            let r = s16(d - base);
            if -(1i64 << (nb - 1)) <= r && r < (1i64 << (nb - 1)) {
                return (code, nb, r & ((1i64 << nb) - 1));
            }
        }
    }
    // unreachable: the last entry (16 extra bits) carries every int16 difference
    ("0000000000", 16, d & 0xFFFF)
}

/// Default-table Huffman of the differences x[i] - x[i-1] (x[-1] = 0, int16 wrap).
pub fn huff_encode(values: &[i16]) -> Vec<u8> {
    let mut w = BitWriter::new();
    let mut prev = 0i64;
    for &x in values {
        let d = s16(x as i64 - prev);
        prev = x as i64;
        let (code, nb, extra) = huff_pick(d);
        w.put_str(code);
        if nb != 0 {
            w.put(extra, nb);
        }
    }
    w.bytes(1)
}

/// Method 0 block: length + plain BE16 values.
pub fn method0_block(vals: &[i16]) -> Vec<u8> {
    with_len(&pack_be16(vals))
}

/// Method 1 block: length + default-table Huffman.
pub fn method1_block(vals: &[i16]) -> Vec<u8> {
    with_len(&huff_encode(vals))
}

/// Owned compressed record (the blocks a [`Cmp`] points into).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EncodedCmp {
    pub method: i16,
    pub count: u16,
    pub main: Option<Vec<u8>>,
    pub leads: [Option<Vec<u8>>; 8],
}

impl EncodedCmp {
    pub fn as_cmp(&self) -> Cmp<'_> {
        let mut leads: [Option<&[u8]>; 8] = [None; 8];
        for (d, s) in leads.iter_mut().zip(self.leads.iter()) {
            *d = s.as_deref();
        }
        Cmp { method: self.method, count: self.count, main: self.main.as_deref(), leads }
    }

    /// Total size of all blocks in bytes.
    pub fn size(&self) -> usize {
        self.main.as_ref().map_or(0, |b| b.len()) + self.leads.iter().flatten().map(|b| b.len()).sum::<usize>()
    }
}

fn col(v: &[i16], i: usize) -> i64 {
    v.get(i).copied().unwrap_or(0) as i64
}

/// Methods 0/1 (lossless). `avg8`: 8 leads x 600 (I, II, V1..V6); `leads8`: 8 x `count`.
pub fn encode_m01(method: i16, avg8: &[Vec<i16>], leads8: &[Vec<i16>], count: usize) -> EncodedCmp {
    let enc = |v: &[i16]| if method == 0 { pack_be16(v) } else { huff_encode(v) };
    let flat: Vec<i16> = (0..8).flat_map(|l| (0..600).map(move |k| col(lead(avg8, l), k) as i16)).collect();
    let mut out = EncodedCmp { method, count: count as u16, main: Some(with_len(&enc(&flat))), ..Default::default() };
    for (l, slot) in out.leads.iter_mut().enumerate() {
        let v: Vec<i16> = (0..count).map(|i| col(lead(leads8, l), i) as i16).collect();
        *slot = Some(with_len(&enc(&v)));
    }
    out
}

fn lead(v: &[Vec<i16>], l: usize) -> &[i16] {
    v.get(l).map_or(&[], |x| x.as_slice())
}

// ----------------------------------------------------------------------------- methods 4 / 5

/// Expected decoder output according to the encoder's own model.
#[derive(Clone, Debug, Default)]
pub struct Expected {
    /// 8 x 600 average.
    pub avg: Vec<Vec<i16>>,
    /// 8 x N rhythm samples.
    pub leads: Vec<Vec<i16>>,
}

/// Python `round()` (half to even) of a float.
fn pyround(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// Escape: 4 bits a = v + 8 (|v| <= 7); 0000 + 8 bits (v + 8 or v + 263); 0000 0000 + 16 bits.
pub fn m45_escape_bits(w: &mut BitWriter, v: i64) {
    if (-7..=7).contains(&v) {
        w.put(v + 8, 4);
    } else if (8..=127).contains(&v) {
        w.put(0, 4);
        w.put(v + 8, 8);
    } else if (-127..=-8).contains(&v) {
        w.put(0, 4);
        w.put(v + 263, 8);
    } else {
        w.put(0, 8);
        w.put(v & 0xFFFF, 16);
    }
}

/// Average value code: 0 | 1^k 0 s (k <= 6) | 1^7 escape.
fn m4_avg_value(w: &mut BitWriter, d: i64) {
    let k = d.abs();
    if d == 0 {
        w.put(0, 1);
    } else if k <= 6 {
        w.put((1 << (k + 1)) - 2, (k + 1) as u32);
        w.put((d < 0) as i64, 1);
    } else {
        w.put(0x7F, 7);
        m45_escape_bits(w, d);
    }
}

/// Average block of methods 4/5 (`nper` values per lead taken as `src(l, j)`): closed-loop
/// quantisation with step 4 against the 2nd-order predictor. Returns (block, reconstruction).
fn m45_encode_avg(nper: usize, src: impl Fn(usize, usize) -> i64) -> (Vec<u8>, Vec<Vec<i64>>) {
    let mut w = BitWriter::new();
    let mut rec8 = Vec::new();
    for l in 0..8 {
        let mut rec: Vec<i64> = Vec::with_capacity(nper);
        for j in 0..nper {
            let pred = if j < 2 { 0 } else { s16(2 * rec[j - 1] - rec[j - 2]) };
            let d = pyround((src(l, j) - pred) as f64 / 4.0).clamp(-8191, 8191);
            rec.push(s16(4 * d + pred));
            m4_avg_value(&mut w, d);
        }
        rec8.push(rec);
    }
    (with_len(&w.bytes(1)), rec8)
}

/// Method 4 average block (8 x 600). Returns (block, reconstructed 8 x 600).
pub fn m4_encode_avg(avg8: &[Vec<i16>]) -> (Vec<u8>, Vec<Vec<i16>>) {
    let (b, rec) = m45_encode_avg(600, |l, j| col(lead(avg8, l), j));
    (b, to_i16(rec))
}

fn to_i16(v: Vec<Vec<i64>>) -> Vec<Vec<i16>> {
    v.into_iter().map(|x| x.into_iter().map(|y| y as i16).collect()).collect()
}

/// Method 4 lead codes: 0 + escape(r - 8) = run of r zeros, 1^k 0 s (k <= 3), 1111 + escape.
fn m4_lead_bits(w: &mut BitWriter, e_list: &[i64]) {
    let n = e_list.len();
    let mut i = 0;
    while i < n {
        if e_list[i] == 0 {
            let mut r = 1;
            while i + r < n && e_list[i + r] == 0 && r < 135 {
                r += 1;
            }
            w.put(0, 1);
            m45_escape_bits(w, r as i64 - 8);
            i += r;
            continue;
        }
        put_lead_value(w, e_list[i]);
        i += 1;
    }
}

/// 1^k 0 s for |e| <= 3, else 1111 + escape.
fn put_lead_value(w: &mut BitWriter, e: i64) {
    let k = e.abs();
    if k <= 3 {
        w.put((1 << (k + 1)) - 2, (k + 1) as u32);
        w.put((e < 0) as i64, 1);
    } else {
        w.put(0xF, 4);
        m45_escape_bits(w, e);
    }
}

/// Method 5 lead codes: 0 = one zero, otherwise as method 4.
fn m5_lead_bits(w: &mut BitWriter, e_list: &[i64]) {
    for &e in e_list {
        if e == 0 {
            w.put(0, 1);
        } else {
            put_lead_value(w, e);
        }
    }
}

/// Model of the decoder's in-place x2 linear upsampling (`count` rows from count/2 values).
pub fn upsample_m4(h: &[i64], count: usize) -> Vec<i64> {
    let mut y = vec![0i64; count];
    let m = h.len();
    if m == 0 || count == 0 {
        return y;
    }
    let k = m.min(count);
    y[..k].copy_from_slice(&h[..k]);
    y[count - 1] = h[m - 1];
    let mut dst = count as i64 - 3;
    for k in (1..m).rev() {
        let (a, b) = (h[k], h[k - 1]);
        if dst + 1 >= 0 && ((dst + 1) as usize) < count {
            y[(dst + 1) as usize] = s16((3 * a + b) >> 2);
        }
        if dst >= 0 && (dst as usize) < count {
            y[dst as usize] = s16((3 * b + a) >> 2);
        }
        dst -= 2;
    }
    y
}

/// Half-rate closed-loop quantisation of `r` (count values): returns (codes, integrated values).
fn m45_half_rate(r: &[f64], count: usize) -> (Vec<i64>, Vec<i64>) {
    let m = count / 2;
    let mut h = Vec::with_capacity(m);
    let mut e_list = Vec::with_capacity(m);
    let mut prev = 0i64;
    for k in 0..m {
        let target = (r[2 * k] + r[2 * k + 1]) / 2.0;
        let e = pyround((target - prev as f64) / 4.0).clamp(-8191, 8191);
        prev = s16(prev + 4 * e);
        h.push(prev);
        e_list.push(e);
    }
    (e_list, h)
}

/// Method 4 (lossy). `template`: optional 8 x `count` contribution the decoder will add (decode a
/// zero stream to get it); the encoder codes target/2 - template.
pub fn encode_m4(
    avg8: &[Vec<i16>],
    leads8: &[Vec<i16>],
    count: usize,
    template: Option<&[Vec<i16>]>,
) -> (EncodedCmp, Expected) {
    let (main, avg_rec) = m4_encode_avg(avg8);
    let mut out = EncodedCmp { method: 4, count: count as u16, main: Some(main), ..Default::default() };
    let mut exp_leads = Vec::new();
    for l in 0..8 {
        let y = lead(leads8, l);
        let tpl: Vec<i64> = (0..count).map(|i| template.map_or(0, |t| col(lead(t, l), i))).collect();
        // (row 0 is not doubled: it comes out at half size)
        let r: Vec<f64> = (0..count).map(|i| col(y, i) as f64 / 2.0 - tpl[i] as f64).collect();
        let (e_list, h) = m45_half_rate(&r, count);
        let mut w = BitWriter::new();
        m4_lead_bits(&mut w, &e_list);
        out.leads[l] = Some(with_len(&w.bytes(1)));
        let up = upsample_m4(&h, count);
        let mut o: Vec<i64> = (0..count).map(|i| s16(up[i] + tpl[i])).collect();
        for v in o.iter_mut().skip(1) {
            *v = s16(*v << 1);
        }
        exp_leads.push(o.into_iter().map(|v| v as i16).collect());
    }
    (out, Expected { avg: avg_rec, leads: exp_leads })
}

/// Method 5 (lossy, no template). `avg8`: 8 x 600 final average (even samples are coded);
/// `leads8`: 8 x 2*`count` final rhythm. Expected output = Catmull-Rom of the decoder model.
pub fn encode_m5(avg8: &[Vec<i16>], leads8: &[Vec<i16>], count: usize) -> (EncodedCmp, Expected) {
    let (main, rec300) = m45_encode_avg(300, |l, j| col(lead(avg8, l), 2 * j));
    let exp_avg = rec300.iter().map(|r| super::m45::catmull_up2(r, 128).into_iter().map(|v| v as i16).collect()).collect();
    let mut out = EncodedCmp { method: 5, count: count as u16, main: Some(main), ..Default::default() };
    let mut exp_leads = Vec::new();
    for l in 0..8 {
        let y = lead(leads8, l);
        // final = Catmull-Rom gain 2 of the count-rate rows, so code half of every other sample
        let r: Vec<f64> = (0..count).map(|i| col(y, 2 * i) as f64 / 2.0).collect();
        let (e_list, h) = m45_half_rate(&r, count);
        let mut w = BitWriter::new();
        m5_lead_bits(&mut w, &e_list);
        out.leads[l] = Some(with_len(&w.bytes(1)));
        let up = upsample_m4(&h, count);
        exp_leads.push(super::m45::catmull_up2(&up, 64).into_iter().map(|v| v as i16).collect());
    }
    (out, Expected { avg: exp_avg, leads: exp_leads })
}

// ----------------------------------------------------------------------------- method 16

/// Value -> (symbol, extra bits value) with the fixed value table.
pub fn m16_symbol(v: i64) -> (usize, i64) {
    let v = s16(v);
    if v == 0 {
        return (0, 0);
    }
    if (-12..=12).contains(&v) {
        return (if v > 0 { 2 * v as usize } else { 2 * (-v) as usize - 1 }, 0);
    }
    if v == -13 {
        return (25, 0);
    }
    for s in 26..32 {
        let lo = M16_BASE[s];
        let hi = lo + (1i64 << M16_EXTRA[s]) - 1;
        if lo <= v && v <= hi {
            return (s, v - lo);
        }
    }
    (32, v & 0xFFFF)
}

/// Huffman code lengths for all 33 symbols (every symbol gets a code), same tie-breaking as the
/// Python `heapq` version: (frequency, creation id).
pub fn huffman_lengths(freq: &[u64]) -> Result<Vec<u32>, String> {
    let f: Vec<u64> = freq.iter().map(|&x| x.max(1)).collect();
    let mut groups: Vec<Vec<usize>> = (0..f.len()).map(|i| vec![i]).collect();
    let mut heap: BinaryHeap<Reverse<(u64, usize)>> = (0..f.len()).map(|i| Reverse((f[i], i))).collect();
    let mut lens = vec![0u32; f.len()];
    while heap.len() > 1 {
        let (Some(Reverse(a)), Some(Reverse(b))) = (heap.pop(), heap.pop()) else { break };
        let mut syms = groups[a.1].clone();
        syms.extend_from_slice(&groups[b.1]);
        for &s in &syms {
            lens[s] += 1;
        }
        let uid = groups.len();
        groups.push(syms);
        heap.push(Reverse((a.0 + b.0, uid)));
    }
    if lens.iter().any(|&l| l > 32) {
        return Err("code longer than 32 bits".into());
    }
    Ok(lens)
}

/// Canonical prefix code, left-aligned in 32 bits like the code table stores them.
pub fn canonical_codes(lens: &[u32]) -> Vec<u64> {
    let mut order: Vec<usize> = (0..lens.len()).collect();
    order.sort_by_key(|&s| (lens[s], s));
    let mut codes = vec![0u64; lens.len()];
    let mut code = 0u64;
    let mut prev_len = order.first().map_or(0, |&s| lens[s]);
    for &s in &order {
        code <<= lens[s] - prev_len;
        prev_len = lens[s];
        codes[s] = code;
        code += 1;
    }
    codes.iter().enumerate().map(|(s, &c)| (c << (32 - lens[s].min(32))) & 0xFFFF_FFFF).collect()
}

/// Code table section: BE16 byte length + 33 x {u8 L, ceil(L/8) bytes of left-aligned code} + pad.
pub fn m16_table(lens: &[u32], codes: &[u64]) -> Vec<u8> {
    let mut raw = vec![0u8, 0u8];
    for s in 0..33 {
        let l = lens[s];
        raw.push(l as u8);
        for k in 0..l.div_ceil(8) {
            raw.push(((codes[s] >> (24 - 8 * k as i64).max(0)) & 0xFF) as u8); // k <= 3 for L <= 32
        }
    }
    if raw.len() & 1 != 0 {
        raw.push(0);
    }
    let n = raw.len() as i64;
    raw[0..2].copy_from_slice(&be16b(n));
    raw
}

/// BE32 bit count (incl. the 32 header bits) + MSB-first codes, padded to whole 16-bit words.
pub fn m16_segment(values: &[i64], lens: &[u32], codes: &[u64]) -> Vec<u8> {
    let mut w = BitWriter::new();
    for &v in values {
        let (s, extra) = m16_symbol(v);
        w.put((codes[s] >> 32u32.saturating_sub(lens[s])) as i64, lens[s]);
        if M16_EXTRA[s] != 0 {
            w.put(extra, M16_EXTRA[s]);
        }
    }
    let mut out = be32b(32 + w.total as i64).to_vec();
    out.extend(w.bytes(2));
    out
}

/// Code table + data section for a list of segments (each a list of coded values).
pub fn m16_data(segments: &[Vec<i64>]) -> Result<Vec<u8>, String> {
    let mut freq = [0u64; 33];
    for seg in segments {
        for &v in seg {
            freq[m16_symbol(v).0] += 1;
        }
    }
    let lens = huffman_lengths(&freq)?;
    let codes = canonical_codes(&lens);
    let mut out = m16_table(&lens, &codes);
    let segs: Vec<u8> = segments.iter().flat_map(|s| m16_segment(s, &lens, &codes)).collect();
    out.extend(with_len(&segs));
    Ok(out)
}

fn varint_bits(w: &mut BitWriter, gap: i64) -> Result<(), String> {
    match gap {
        1..=8 => {
            w.put(0, 1);
            w.put(gap - 1, 3);
        }
        9..=72 => {
            w.put(0b10, 2);
            w.put(gap - 9, 6);
        }
        73..=584 => {
            w.put(0b110, 3);
            w.put(gap - 73, 9);
        }
        585..=4680 => {
            w.put(0b1110, 4);
            w.put(gap - 585, 12);
        }
        4681..=70216 => {
            w.put(0b1111, 4);
            w.put(gap - 4681, 16);
        }
        _ => return Err(format!("gap {gap} not codable")),
    }
    Ok(())
}

/// Flag section from per-record flag words (record 0 = initial state).
pub fn m16_flags(words: &[u16], nch: usize, count: usize) -> Result<Vec<u8>, String> {
    let init = words.first().copied().unwrap_or(0) as i64;
    let wd = |r: usize| words.get(r).copied().unwrap_or(0) as i64;
    let mut out = Vec::new();
    out.extend_from_slice(&be16b(0));
    out.extend_from_slice(&be16b(init));
    for &bit in FLAG_BIT.iter().take(nch) {
        let toggles: Vec<i64> = (1..count).filter(|&r| (wd(r) ^ wd(r - 1)) & bit != 0).map(|r| r as i64).collect();
        if toggles.len() > 1498 {
            return Err("too many toggles".into());
        }
        let mut w = BitWriter::new();
        let mut prev = 0;
        for &p in &toggles {
            varint_bits(&mut w, p - prev)?;
            prev = p;
        }
        out.extend_from_slice(&be16b(16 + w.total as i64));
        out.extend(w.bytes(2));
    }
    let n = out.len() as i64;
    out[0..2].copy_from_slice(&be16b(n));
    Ok(out)
}

/// Bits that are not channel bits must stay as in record 0.
pub fn m16_check_flag_words(words: &[u16], nch: usize) -> bool {
    let mask: i64 = FLAG_BIT.iter().take(nch).fold(0, |a, &b| a | b);
    let w0 = words.first().copied().unwrap_or(0) as i64;
    words.iter().all(|&w| ((w as i64 ^ w0) & !mask & 0xFFFF) == 0)
}

/// Final average of one lead -> coded values on [x0, x1] (decoder: cumsum, cumsum, x2).
pub fn m16_avg_values(a: &[i16], x0: usize, x1: usize) -> Vec<i64> {
    let b: Vec<i64> = (x0..=x1).map(|k| s16(col(a, k) >> 1)).collect();
    let mut c = vec![b[0]];
    c.extend((1..b.len()).map(|j| s16(b[j] - b[j - 1])));
    let mut d = vec![c[0]];
    d.extend((1..c.len()).map(|j| s16(c[j] - c[j - 1])));
    d
}

/// Segment of a method-16 plan: (type 0/1, param 0/1, number of coded values).
pub type PlanSeg = (u8, u8, usize);

/// Coded values per segment and the samples the decoder will produce for one lead.
pub fn m16_place(plan: &[PlanSeg], s: &[i16], count: usize) -> Result<(Vec<Vec<i64>>, Vec<i16>), String> {
    let sv = |i: usize| s16(col(s, i) >> 3);
    let mut v: Vec<i64> = Vec::new();
    let mut seg_n = Vec::new();
    let mut out = vec![0i64; count];
    let set = |out: &mut Vec<i64>, i: usize, x: i64| -> Result<(), String> {
        *out.get_mut(i).ok_or("plan runs past count")? = x;
        Ok(())
    };
    let mut pos = 0usize;
    for &(typ, param, n) in plan {
        let vals: Vec<i64>;
        match typ {
            0 => {
                vals = (0..n).map(|i| sv(pos + i)).collect();
                for (i, &x) in vals.iter().enumerate() {
                    set(&mut out, pos + i, s16(x << 3))?;
                }
                pos += n;
            }
            1 if n <= 4 => {
                vals = (0..n).map(|i| sv(pos + i)).collect();
                for (i, &x) in vals.iter().enumerate() {
                    set(&mut out, pos + i, s16(x << 3))?;
                }
                pos += n;
            }
            1 => {
                let m = n - 4;
                let mut vv: Vec<i64> = (0..m).map(|i| sv(pos + 2 * i)).collect();
                let base = pos + 2 * m - 1 + param as usize;
                vv.extend((0..4).map(|j| sv(base + j)));
                for (i, &x) in vv.iter().take(m).enumerate() {
                    set(&mut out, pos + 2 * i, s16(x << 3))?;
                }
                for i in 1..m {
                    let mid = s16(cdiv(out[pos + 2 * i - 2] + out[pos + 2 * i], 2));
                    set(&mut out, pos + 2 * i - 1, mid)?;
                }
                for j in 0..4 {
                    set(&mut out, base + j, s16(vv[m + j] << 3))?;
                }
                if param == 1 {
                    let mid = s16(cdiv(out[base - 2] + out[base], 2));
                    set(&mut out, base - 1, mid)?;
                }
                pos = base + 4;
                vals = vv;
            }
            _ => return Err("encoder only does segment types 0 and 1".into()),
        }
        v.extend(vals);
        seg_n.push(n);
    }
    if pos != count {
        return Err(format!("plan covers {pos} samples, count is {count}"));
    }
    // coded numbers: differences of the running values, continuous over segment borders
    let d: Vec<i64> = (0..v.len()).map(|i| if i == 0 { v[0] } else { s16(v[i] - v[i - 1]) }).collect();
    let mut segs = Vec::new();
    let mut k = 0;
    for n in seg_n {
        segs.push(d[k..k + n].to_vec());
        k += n;
    }
    Ok((segs, out.into_iter().map(|x| x as i16).collect()))
}

/// Full-rate chunks of at most `seg` samples.
pub fn default_plan(count: usize, seg: usize) -> Vec<PlanSeg> {
    let mut plan = Vec::new();
    let mut left = count;
    while left > 0 {
        let n = seg.min(left);
        plan.push((0, 0, n));
        left -= n;
    }
    plan
}

/// Expected output of [`encode_m16`].
#[derive(Clone, Debug, Default)]
pub struct Expected16 {
    pub avg: Vec<Vec<i16>>,
    pub leads: Vec<Vec<i16>>,
    /// Flag words of record columns 1, 2, 3.
    pub flags: [Vec<u16>; 3],
}

/// Method 16 with in-stream Huffman tables.
/// `avg8`: 8 x 600 final average (only [x0, x1] used; exact when even there);
/// `leads8`: 8 x `count` rhythm (exact at multiples of 8); `flags`: words of columns 1, 2, 3
/// (default zeros; column 1 has 9 channels, 2/3 have 8); `plan`: segment plan shared by the leads
/// (default full-rate chunks of <= 4000).
pub fn encode_m16(
    avg8: &[Vec<i16>],
    leads8: &[Vec<i16>],
    count: usize,
    x0: u16,
    x1: u16,
    flags: Option<&[Vec<u16>; 3]>,
    plan: Option<&[PlanSeg]>,
) -> Result<(EncodedCmp, Expected16), String> {
    let (x0, x1) = (x0 as usize, x1 as usize);
    let dplan;
    let plan = match plan {
        Some(p) => p,
        None => {
            dplan = default_plan(count, 4000);
            &dplan
        }
    };
    let fw: [Vec<u16>; 3] = match flags {
        Some(f) => f.clone(),
        None => [vec![0; count], vec![0; count], vec![0; count]],
    };
    for (c, nch) in [(0usize, 9usize), (1, 8), (2, 8)] {
        if !m16_check_flag_words(&fw[c], nch) {
            return Err(format!("flag column {} changes non-channel bits", c + 1));
        }
    }
    let mut exp_avg = vec![vec![0i16; 600]; 8];
    let mut body;
    if x0 < x1 && x1 < 600 {
        let mut segs = Vec::new();
        for (l, e) in exp_avg.iter_mut().enumerate() {
            segs.push(m16_avg_values(lead(avg8, l), x0, x1));
            for (k, slot) in e.iter_mut().enumerate().take(x1 + 1).skip(x0) {
                *slot = s16((col(lead(avg8, l), k) >> 1) << 1) as i16;
            }
        }
        body = m16_data(&segs)?;
    } else {
        body = be16b(2).to_vec(); // empty table and empty data section
        body.extend(with_len(&[]));
    }
    body.extend(m16_flags(&fw[0], 9, count)?);
    let main = with_len(&body);
    if main.len() > 12000 {
        return Err("main block longer than 12000 bytes".into());
    }
    let nb = 4 * plan.len();
    if !(1..=0x1800).contains(&nb) {
        return Err("1..1536 segments allowed".into());
    }
    let mut dw = BitWriter::new();
    for &(typ, param, _) in plan {
        dw.put(typ as i64, 2);
        dw.put(param as i64, 2);
    }
    let mut desc = be16b(nb as i64).to_vec();
    desc.extend(dw.bytes(2));
    let mut out = EncodedCmp { method: 16, count: count as u16, main: Some(main), ..Default::default() };
    let mut exp_leads = Vec::new();
    for l in 0..8 {
        let (segs, o) = m16_place(plan, lead(leads8, l), count)?;
        let mut body = if l == 0 { desc.clone() } else { Vec::new() };
        body.extend(m16_data(&segs)?);
        if l < 2 {
            body.extend(m16_flags(&fw[1 + l], 8, count)?);
        }
        let blk = with_len(&body);
        if blk.len() / 2 > count {
            return Err(format!("lead {l} block has {} words > count {count}", blk.len() / 2));
        }
        out.leads[l] = Some(blk);
        exp_leads.push(o);
    }
    Ok((out, Expected16 { avg: exp_avg, leads: exp_leads, flags: fw }))
}
