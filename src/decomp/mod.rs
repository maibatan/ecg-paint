//! Decoder for the compressed 12-lead ECG of t_ecaps12c.dll (Decomp12lead, MedDecomp12lead,
//! DecompressWave).
//!
//! Port of `research/py/decomp_decode.py` (the reference), function by function. Buffers are
//! modelled as flat int16 word arrays exactly like the Python `Machine`, so the in-place quirks of
//! the original (26-byte work rows inside the 32-byte record buffer, the stride-28 zero fill of
//! method 4, row 0 not doubled, ...) give the same numbers.
//!
//! Integer semantics follow the Python: all arithmetic is done in `i64`, `s16`/`u16` wrap where the
//! reference wraps, `>>` is an arithmetic shift and division truncates toward zero (C `cdiv`).
//! Reads outside a buffer return 0 and writes outside a buffer are dropped (the Python `be16`,
//! `Machine.rget`/`rset`). No input can make this module panic or loop forever.

mod m01;
mod m45;
mod m16;
pub mod encode;

// ----------------------------------------------------------------------------- public types

/// Average-beat fiducial points (`F`, meas+0, first 14-byte entry).
/// `w0 == 0xFFFF` means "absent" (only consulted by method 5 to decide whether `w2` is doubled).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fid {
    pub w0: u16,
    pub w2: u16,
    pub p4: u16,
    pub p6: u16,
    pub p8: u16,
    pub pa: u16,
    pub pc: u16,
}

/// The measurement data the decoder reads/rewrites (header `MH` + lists).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meas {
    /// MH+0: average window start.
    pub x0: u16,
    /// MH+2: average window end.
    pub x1: u16,
    pub f: Fid,
    /// QRS list: (position, b2); terminated by position 0xFFFF (or the end of the Vec).
    pub qrs: Vec<(u16, u8)>,
    /// Second list (positions, byte 1 of the record = 0x80); terminated by 0xFFFF.
    pub list2: Vec<u16>,
}

/// Compressed input. `main`/`leads` start at the block's BE16 length field and run to the end of
/// the containing buffer (reads past the block's own length see the following bytes, like the DLL).
#[derive(Clone, Copy, Debug)]
pub struct Cmp<'a> {
    pub method: i16,
    pub count: u16,
    pub main: Option<&'a [u8]>,
    pub leads: [Option<&'a [u8]>; 8],
}

/// Decoder output.
#[derive(Clone, Debug)]
pub struct DecompResult {
    /// Return value of the DLL function (int16).
    pub status: i16,
    /// Number of valid records (method 5: 2 x count).
    pub count: usize,
    /// `count` records of 16 words: word 0 = flag bytes (b0 | b1 << 8), words 1..3 = method-16
    /// flag columns, 4..11 = I, II, V1..V6, 12..15 = III, aVR, aVL, aVF.
    pub records: Vec<[i16; 16]>,
    /// Average beat: 12 leads x 600 samples in storage order (I, II, V1..V6, III, aVR, aVL, aVF).
    pub avg: Vec<[i16; 600]>,
    /// The u16 header word in front of the average buffer (0xD602 / 0x3842 / 0).
    pub avg_header: i16,
    /// Measurement data after decoding (method 5 rewrites the positions).
    pub meas: Meas,
}

/// Largest wave capacity that can matter (method 5 needs 2 x 65535 records); larger `cap`
/// values are clamped to this so the allocation stays bounded.
pub const MAX_CAP: usize = 0x20000;

// ----------------------------------------------------------------------------- helpers

/// Wrap to a signed 16-bit value.
#[inline]
pub(crate) fn s16(x: i64) -> i64 {
    x as i16 as i64
}

/// Wrap to an unsigned 16-bit value.
#[inline]
pub(crate) fn u16w(x: i64) -> i64 {
    x as u16 as i64
}

/// C integer division (truncates toward zero). `b` is never 0 at the call sites.
#[inline]
pub(crate) fn cdiv(a: i64, b: i64) -> i64 {
    if b == 0 {
        0
    } else {
        a / b
    }
}

/// Big-endian u16 at byte `off`; a read that runs off the end returns the bytes that exist
/// (high byte only) or 0.
#[inline]
pub(crate) fn be16(b: &[u8], off: i64) -> i64 {
    if off < 0 {
        return 0;
    }
    let o = off as usize;
    if o.saturating_add(1) < b.len() {
        ((b[o] as i64) << 8) | b[o + 1] as i64
    } else if o < b.len() {
        (b[o] as i64) << 8
    } else {
        0
    }
}

/// Python slice `b[a:e]` (clamped, never panics).
#[inline]
pub(crate) fn pyslice(b: &[u8], a: usize, e: usize) -> &[u8] {
    let e = e.min(b.len());
    let a = a.min(e);
    &b[a..e]
}

/// Flat int16 memory: the wave buffer (`cap` records x 16 words) and the average buffer
/// (1 header word + 12 x 600 words), each with the same slack as the Python model.
pub(crate) struct Machine {
    pub cap: usize,
    pub wave: Vec<i16>,
    pub avg: Vec<i16>,
}

const SLACK: usize = 64;

impl Machine {
    fn new(cap: usize) -> Machine {
        Machine { cap, wave: vec![0; cap * 16 + SLACK], avg: vec![0; 1 + 12 * 600 + SLACK] }
    }

    #[inline]
    pub fn rget(&self, w: i64) -> i64 {
        if w >= 0 && (w as u64) < self.wave.len() as u64 {
            self.wave[w as usize] as i64
        } else {
            0
        }
    }

    #[inline]
    pub fn rset(&mut self, w: i64, v: i64) {
        if w >= 0 && (w as u64) < self.wave.len() as u64 {
            self.wave[w as usize] = v as i16;
        }
    }

    #[inline]
    pub fn aget(&self, i: i64) -> i64 {
        if i >= 0 && (i as u64) < self.avg.len() as u64 {
            self.avg[i as usize] as i64
        } else {
            0
        }
    }

    #[inline]
    pub fn aset(&mut self, i: i64, v: i64) {
        if i >= 0 && (i as u64) < self.avg.len() as u64 {
            self.avg[i as usize] = v as i16;
        }
    }
}

// ----------------------------------------------------------------------------- default Huffman table

/// The 33-entry default table (DLL 0x10019e88): (code bits, base, extra bits). The extra bits are
/// a signed (two's complement) offset added to `base`.
pub const DEFAULT_TABLE: [(&str, i16, u8); 33] = [
    ("01", 0, 0),
    ("111", 1, 0),
    ("110", -1, 0),
    ("001", 2, 0),
    ("1011", -2, 0),
    ("0001", 3, 0),
    ("10101", -3, 0),
    ("10010", 4, 0),
    ("00001", -4, 0),
    ("100110", 5, 0),
    ("000001", -5, 0),
    ("1000111", 6, 0),
    ("1000110", -6, 0),
    ("10100001", 7, 0),
    ("0000001", -7, 0),
    ("10000101", 8, 0),
    ("10001011", -8, 0),
    ("10000110", 9, 0),
    ("00000001", -9, 0),
    ("10000111", 10, 0),
    ("100010100", -10, 0),
    ("100010101", 11, 0),
    ("100001000", -11, 0),
    ("100001001", 12, 0),
    ("000000001", -12, 0),
    ("0000000001", -13, 0),
    ("101001", 21, 4),
    ("1000100", -21, 4),
    ("1010001", 37, 4),
    ("10100000", -37, 4),
    ("100111", 173, 8),
    ("100000", -173, 8),
    ("0000000000", 0, 16),
];

/// Virtual address of the default table inside t_ecaps12c.dll (for cross-checking).
pub const DEFAULT_TABLE_VA: u32 = 0x10019E88;

/// (length, value) of a '0'/'1' code string.
fn code_bits(code: &str) -> (u32, u32) {
    let mut v = 0u32;
    for ch in code.bytes() {
        v = (v << 1) | (ch == b'1') as u32;
    }
    (code.len() as u32, v)
}

/// MSB-first bit source of the Huffman decoder: reading past the last byte is an error.
struct HuffBits<'a> {
    src: &'a [u8],
    i: usize,
    bit: i32,
}

impl HuffBits<'_> {
    fn get(&mut self) -> Option<i64> {
        if self.bit < 0 {
            self.i += 1;
            if self.i >= self.src.len() {
                return None;
            }
            self.bit = 7;
        }
        let byte = self.src.get(self.i).copied().unwrap_or(0);
        let b = (byte >> self.bit) & 1;
        self.bit -= 1;
        Some(b as i64)
    }
}

/// Huffman + running sum with the default table (method 1, DecompressWave mode 3).
/// Returns (values, status): 0 = ok, -1 = ran out of data / unknown bit pattern / count 0;
/// values not decoded stay 0.
pub fn huff_decode(src: &[u8], count: usize) -> (Vec<i16>, i32) {
    let mut out: Vec<i16> = Vec::new();
    if out.try_reserve_exact(count).is_err() {
        return (Vec::new(), -1);
    }
    out.resize(count, 0);
    if count == 0 {
        return (out, -1);
    }
    let table: Vec<(u32, u32, i64, u32)> = DEFAULT_TABLE
        .iter()
        .map(|&(c, base, nb)| {
            let (len, val) = code_bits(c);
            (len, val, base as i64, nb as u32)
        })
        .collect();
    let maxlen = table.iter().map(|e| e.0).max().unwrap_or(0);
    let mut bits = HuffBits { src, i: 0, bit: 7 };
    let mut prev = 0i64;
    for slot in out.iter_mut() {
        let mut len = 0u32;
        let mut val = 0u32;
        // grow the code one bit at a time until it is in the table (a later duplicate wins)
        let (base, nb) = loop {
            let Some(b) = bits.get() else { return (out, -1) };
            val = (val << 1) | b as u32;
            len += 1;
            if let Some(e) = table.iter().rev().find(|e| e.0 == len && e.1 == val) {
                break (e.2, e.3);
            }
            if len > maxlen {
                return (out, -1);
            }
        };
        let d = if nb > 0 {
            let mut v = 0i64;
            for _ in 0..nb {
                let Some(b) = bits.get() else { return (out, -1) };
                v = (v << 1) | b;
            }
            if v & (1 << (nb - 1)) != 0 {
                v -= 1 << nb;
            }
            s16(base + v)
        } else {
            base
        };
        prev = s16(prev + d);
        *slot = prev as i16;
    }
    // (the loop above writes in order, so on an early return the tail is still 0)
    (out, 0)
}

/// DecompressWave non-Huffman mode: plain BE16 values; `src` must be exactly 2 x count bytes
/// (otherwise all zeros and status -2).
pub fn unpack_be16(src: &[u8], count: usize) -> (Vec<i16>, i32) {
    if Some(src.len()) != count.checked_mul(2) {
        let mut v = Vec::new();
        if v.try_reserve_exact(count).is_ok() {
            v.resize(count, 0);
        }
        return (v, -2);
    }
    let v = (0..count).map(|i| be16(src, 2 * i as i64) as i16).collect();
    (v, 0)
}

/// What EcgPaint's DecompressWave wrapper does: mode 3 with the default table.
pub fn decompress_wave(src: &[u8], count: usize) -> (Vec<i16>, i32) {
    huff_decode(src, count)
}

// ----------------------------------------------------------------------------- shared steps

/// III, aVR, aVL, aVF for the first `nrec` records (words 12..15).
fn derived_rhythm(m: &mut Machine, nrec: usize) {
    for r in 0..nrec as i64 {
        let i = m.rget(16 * r + 4);
        let ii = m.rget(16 * r + 5);
        m.rset(16 * r + 12, s16(ii - i));
        m.rset(16 * r + 13, s16((-(i + ii)) >> 1));
        m.rset(16 * r + 14, s16(i - (ii >> 1)));
        m.rset(16 * r + 15, s16(ii - (i >> 1)));
    }
}

/// The same four derived leads for the average (leads 8..11).
fn derived_avg(m: &mut Machine) {
    for k in 0..600i64 {
        let i = m.aget(1 + k);
        let ii = m.aget(1 + 600 + k);
        m.aset(1 + 8 * 600 + k, s16(ii - i));
        m.aset(1 + 9 * 600 + k, s16((-(i + ii)) >> 1));
        m.aset(1 + 10 * 600 + k, s16(i - (ii >> 1)));
        m.aset(1 + 11 * 600 + k, s16(ii - (i >> 1)));
    }
}

/// Word 0 of the first `count` records: cleared, then byte 0 = (b2&3)+1 at each QRS (max 256
/// entries) and byte 1 = 0x80 at each list-2 position (max 512 entries).
fn set_record_flags(m: &mut Machine, count: usize, meas: &Meas) {
    for r in 0..count as i64 {
        m.rset(16 * r, 0);
    }
    for &(pos, b2) in meas.qrs.iter().take(256) {
        if pos == 0xFFFF {
            break;
        }
        if (pos as usize) < count {
            let w = 16 * pos as i64;
            m.rset(w, (m.rget(w) & 0xFF00) | (((b2 as i64 & 3) + 1) & 0xFF));
        }
    }
    for &pos in meas.list2.iter().take(512) {
        if pos == 0xFFFF {
            break;
        }
        if (pos as usize) < count {
            let w = 16 * pos as i64;
            m.rset(w, s16((m.rget(w) & 0x00FF) | 0x8000));
        }
    }
}

/// QRS entries up to the 0xFFFF terminator (no 256 limit, like the reference).
fn qrs_entries(meas: &Meas) -> Vec<(u16, u8)> {
    meas.qrs.iter().copied().take_while(|&(p, _)| p != 0xFFFF).collect()
}

// ----------------------------------------------------------------------------- entry points

fn finish(m: &mut Machine, status: i64, count: usize, meas: Meas, ok_derived: bool, nrec: usize) -> DecompResult {
    if ok_derived {
        derived_rhythm(m, nrec);
        derived_avg(m);
    }
    let records = (0..count as i64)
        .map(|r| {
            let mut rec = [0i16; 16];
            for (c, slot) in rec.iter_mut().enumerate() {
                *slot = m.rget(16 * r + c as i64) as i16;
            }
            rec
        })
        .collect();
    let avg = (0..12i64)
        .map(|l| {
            let mut a = [0i16; 600];
            for (k, slot) in a.iter_mut().enumerate() {
                *slot = m.aget(1 + 600 * l + k as i64) as i16;
            }
            a
        })
        .collect();
    DecompResult { status: status as i16, count, records, avg, avg_header: m.avg[0], meas }
}

/// Decomp12lead(wave, meas, cmp): rhythm records and the average beat.
/// `cap` = wave capacity in records (EcgPaint: 12000); clamped to [`MAX_CAP`].
pub fn decomp12lead(cmp: &Cmp, meas: &Meas, cap: usize) -> DecompResult {
    let cap = cap.min(MAX_CAP);
    let mut meas = meas.clone();
    let mut m = Machine::new(cap);
    let method = cmp.method;
    let mut count = cmp.count as usize;
    let mut nrec = cap; // derived leads run over wave+4 = capacity for methods 0/1/4/5
    let st: i64 = match method {
        0 => m01::m01(&mut m, cmp, &meas, false, false),
        1 => m01::m01(&mut m, cmp, &meas, true, false),
        4 => m45::m4(&mut m, cmp, &meas),
        5 => match m45::m5(&mut m, cmp, &mut meas) {
            Ok(n2) => {
                count = n2;
                0
            }
            Err(e) => e,
        },
        16 => {
            nrec = count; // method 16 overwrites wave+4 with count
            m16::m16(&mut m, cmp, &meas)
        }
        _ => -3,
    };
    let is45 = method == 4 || method == 5;
    if st == -32 || (is45 && st == -1) || st == -64 || st == -3 {
        // (a method 0/1 status of -3 = both error bits also lands here, as in the reference)
        if st == -32 || st == -64 || st == -3 || is45 {
            count = 0;
        }
    }
    let nrec = nrec.min(cap);
    finish(&mut m, s16(st), count, meas, st == 0, nrec)
}

/// MedDecomp12lead(meas, cmp): average beat only (records empty, count 0).
pub fn med_decomp12lead(cmp: &Cmp, meas: &Meas) -> DecompResult {
    let mut meas = meas.clone();
    let mut m = Machine::new(16);
    let st: i64 = match cmp.method {
        0 | 1 => m01::m01(&mut m, cmp, &meas, cmp.method == 1, true),
        4 => {
            m45::m45_avg(&mut m, cmp.main, 600);
            m.avg[0] = 0x3842;
            0
        }
        5 => {
            m45::m5_pre(&mut meas);
            m45::m45_avg(&mut m, cmp.main, 300);
            m45::m5_double_meas(&mut meas);
            m45::m5_avg_up(&mut m);
            m.avg[0] = 0x3842;
            0
        }
        16 => {
            m.avg[0] = 0x3842;
            let (x0, x1) = (meas.x0 as i64, meas.x1 as i64);
            let st = if x0 < x1 {
                m16::m16_avg(&mut m, cmp, x0, x1)
            } else if m16::Words::new(cmp.main).w(1) == 2 {
                0
            } else {
                -2
            };
            if st == -2 {
                -1
            } else {
                st
            }
        }
        _ => -2,
    };
    finish(&mut m, s16(st), 0, meas, st == 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_table_is_complete_and_prefix_free() {
        let kraft: f64 = DEFAULT_TABLE.iter().map(|e| 0.5f64.powi(e.0.len() as i32)).sum();
        assert!((kraft - 1.0).abs() < 1e-12);
        let mut codes: Vec<&str> = DEFAULT_TABLE.iter().map(|e| e.0).collect();
        codes.sort();
        for w in codes.windows(2) {
            assert!(!w[1].starts_with(w[0]), "{} prefixes {}", w[0], w[1]);
        }
    }

    #[test]
    fn catmull_constants() {
        assert_eq!(m45::catmull_up2(&[100; 10], 128), vec![100; 20]);
        assert_eq!(m45::catmull_up2(&[100; 10], 64), vec![200; 20]);
    }

    #[test]
    fn empty_and_garbage_inputs_do_not_panic() {
        let meas = Meas::default();
        for method in [-1i16, 0, 1, 2, 3, 4, 5, 6, 15, 16, 17, i16::MIN, i16::MAX] {
            for count in [0u16, 1, 2, 3, 7, 100, 12000, 65535] {
                let cmp = Cmp { method, count, main: None, leads: [None; 8] };
                let r = decomp12lead(&cmp, &meas, 12000);
                assert!(r.count <= 2 * 65535);
                let _ = med_decomp12lead(&cmp, &meas);
                let junk: Vec<u8> = (0..300u32).map(|i| (i.wrapping_mul(2654435761) >> 7) as u8).collect();
                let cmp = Cmp { method, count, main: Some(&junk), leads: [Some(&junk[1..]); 8] };
                let _ = decomp12lead(&cmp, &meas, 12000);
                let _ = med_decomp12lead(&cmp, &meas);
            }
        }
    }
}
