//! The input container (docs/FORMAT.md §4.1): big-endian, a table of contents of
//! checksummed blocks. Parsing only: bytes → [`Record`]. Errors carry the file offset and
//! the original's internal error code.

mod blocks;

use crate::analtext::{get_anal_text, AnalTables};
use crate::error::{Error, Result};
use crate::model::Record;

/// The reader reads at most 1 MiB into a zero-filled buffer (0x411400); later bytes are lost.
pub const MAXBUF: usize = 0x10_0000;

pub const ERR_FMT: u32 = 0xA000_5000;
pub const ERR_MISSING: u32 = 0xA000_6001;
pub const ERR_CHECKSUM: u32 = 0xA000_6002;
pub const ERR_FLAG: u32 = 0xA000_6003;

/// The file as the reader sees it: reads past the data return 0 like the zero-filled buffer.
pub struct Container<'a> {
    pub data: &'a [u8],
    toc: Vec<(u32, u16)>,
}

impl<'a> Container<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let data = &data[..data.len().min(MAXBUF)];
        let mut c = Container { data, toc: Vec::new() };
        let n = c.be16(0x10) as usize;
        c.toc = (0..n).map(|i| (c.be32(0x12 + 6 * i), c.be16(0x16 + 6 * i))).collect();
        c
    }

    pub fn u8(&self, off: usize) -> u8 {
        self.data.get(off).copied().unwrap_or(0)
    }

    pub fn be16(&self, off: usize) -> u16 {
        u16::from_be_bytes([self.u8(off), self.u8(off.wrapping_add(1))])
    }

    pub fn s16(&self, off: usize) -> i16 {
        self.be16(off) as i16
    }

    pub fn be32(&self, off: usize) -> u32 {
        ((self.be16(off) as u32) << 16) | self.be16(off.wrapping_add(2)) as u32
    }

    /// Bytes `off..off+n`, zero-padded past the end of the data.
    pub fn bytes(&self, off: usize, n: usize) -> Vec<u8> {
        (0..n).map(|i| self.u8(off.wrapping_add(i))).collect()
    }

    /// Everything from `off` to the end of the data (pointer semantics for the decoder).
    pub fn rest(&self, off: usize) -> &'a [u8] {
        self.data.get(off..).unwrap_or(&[])
    }

    /// FIND_BLOCK 0x40eea0: body offset (entry offset + 6) of the first TOC entry with this tag.
    pub fn find(&self, tag: u16) -> Option<usize> {
        self.toc.iter().find(|&&(_, t)| t == tag).map(|&(off, _)| off as usize + 6)
    }

    /// The body of a block with its checksum verified (0x40ef80): sum of the `len` bytes
    /// after the checksum word, mod 65536. Returns (body offset, len).
    pub fn body(&self, tag: u16) -> Result<Option<(usize, usize)>> {
        let Some(p) = self.find(tag) else { return Ok(None) };
        let len = self.be32(p + 2) as usize;
        if p >= MAXBUF || len > MAXBUF || p + 2 + len > MAXBUF {
            return Err(Error::at(p, format!("block {tag:#x}: length {len:#x} runs past 1 MiB ({ERR_MISSING:#x})")));
        }
        let sum = (0..len).fold(0u16, |s, i| s.wrapping_add(self.u8(p + 2 + i) as u16));
        let want = self.be16(p);
        if sum != want {
            return Err(Error::at(
                p,
                format!("block {tag:#x}: checksum {want:#06x} != {sum:#06x} ({ERR_CHECKSUM:#x})"),
            ));
        }
        Ok(Some((p, len)))
    }

    /// A block that must be present.
    pub fn required(&self, tag: u16) -> Result<(usize, usize)> {
        self.body(tag)?
            .ok_or_else(|| Error::at(0x10, format!("block {tag:#x} missing ({ERR_MISSING:#x})")))
    }
}

/// TEXT 0x40f0a0(dst, dstsz, src, srclen, trimR, skipL): copy a text field, converting
/// EUC-JP to Shift-JIS when `mode` is 0. The result is always Shift-JIS (cp932) bytes.
pub fn text(c: &Container, off: usize, srclen: usize, dstsz: usize, trim_r: bool, skip_l: bool, mode: u8) -> Vec<u8> {
    // The destination is a zero-filled C buffer: every character is strcpy'd to its slot, so a
    // zero byte (an unwritten slot, or a zero inside a 2-byte character) ends the string there
    // even though later characters are still stored behind it.
    let mut dst = vec![0u8; dstsz + 2];
    let mut put = |o: usize, pair: &[u8]| {
        for (k, &b) in pair.iter().enumerate() {
            if b == 0 {
                break;
            }
            dst[o + k] = b;
        }
    };
    let mut i = 0;
    if skip_l {
        while i < srclen && c.u8(off + i) == 0x20 {
            i += 1;
        }
    }
    let mut o = 0usize;
    while i < srclen && c.u8(off + i) != 0 && o + 1 < dstsz {
        let ch = c.u8(off + i);
        if ch > 0x80 {
            // lead byte: the next byte is taken even past srclen
            let c2 = c.u8(off + i + 1);
            i += 1;
            match mode {
                0 => {
                    let (a, b) = euc_to_sjis(ch, c2);
                    put(o, &[a, b]);
                    o += 2;
                }
                1 => {
                    put(o, &[ch, c2]);
                    o += 2;
                }
                // other modes: nothing written, one slot skipped (it stays zero)
                _ => o += 1,
            }
        } else {
            put(o, &[ch]);
            o += 1;
        }
        i += 1;
    }
    if trim_r {
        // from the last slot backwards, spaces only: an unwritten slot stops the trimming, so
        // spaces before it survive ("AB " when a skipped 2-byte character follows)
        let mut k = o;
        while k > 0 && dst[k - 1] == 0x20 {
            dst[k - 1] = 0;
            k -= 1;
        }
    }
    let end = dst.iter().position(|&b| b == 0).unwrap_or(dst.len()).min(dstsz);
    dst.truncate(end);
    dst
}

/// EUC-JP pair → Shift-JIS pair (0x404840: subtract 0x8080, then JIS → SJIS).
fn euc_to_sjis(c1: u8, c2: u8) -> (u8, u8) {
    let code = ((((c1 as u32) << 8) + c2 as u32).wrapping_sub(0x8080)) & 0xFFFF;
    let (hi, lo) = (code >> 8, code & 0xFF);
    let mut lo2 = lo + if hi & 1 != 0 { 0x1F } else { 0x7D };
    if lo2 >= 0x7F {
        lo2 += 1;
    }
    // signed arithmetic, as the original (sar): a first byte below 0xA1 (not EUC-JP kanji, e.g.
    // the 0x8E of half-width kana) gives a row below 0x21 and must not wrap around
    let mut hi2 = ((hi as i32 - 0x21) >> 1) + 0x81;
    if hi2 > 0x9F {
        hi2 += 0x40;
    }
    let r = (((hi2 as u32) << 8) | lo2) & 0xFFFF;
    ((r >> 8) as u8, r as u8)
}

/// 0x40eca0: BE16 BCD → (hi, lo) two-digit numbers; an F nibble counts as 0.
pub fn bcd2(v: u16) -> (u32, u32) {
    let d: Vec<u32> = [12, 8, 4, 0]
        .iter()
        .map(|sh| ((v >> sh) & 0xF) as u32)
        .map(|x| if x == 0xF { 0 } else { x })
        .collect();
    (10 * d[0] + d[1], 10 * d[2] + d[3])
}

/// Options of the parse that come from the command line / INI.
#[derive(Clone, Copy, Debug)]
pub struct ParseOptions {
    /// English statement tables (GetAnalText sel 1) instead of Japanese.
    pub english: bool,
}

/// Parse a file the way Load 0x407c70 does (both record types), decode the waves and
/// fill the statement text when the vendor tables are available.
pub fn parse(data: &[u8], opts: ParseOptions, tables: Option<&AnalTables>) -> Result<Record> {
    let c = Container::new(data);
    let mut rec = blocks::parse_record(&c)?;
    if let Some(t) = tables {
        let sel = if opts.english { 1 } else { 0 };
        let (lines, rec_text) = get_anal_text(t, sel, &rec.analysis.codes, rec.analysis.class_code);
        rec.analysis.lines = lines;
        rec.analysis.recommendation = rec_text;
    }
    Ok(rec)
}

#[cfg(test)]
pub(crate) mod testutil {
    /// len = size − 2 at +2, checksum of the len bytes from +2 at +0.
    pub fn seal(mut b: Vec<u8>) -> Vec<u8> {
        let n = (b.len() - 2) as u32;
        b[2..6].copy_from_slice(&n.to_be_bytes());
        let s = b[2..].iter().fold(0u16, |s, &x| s.wrapping_add(x as u16));
        b[0..2].copy_from_slice(&s.to_be_bytes());
        b
    }

    /// A container laid out like the program's own writer: prefix, header block with the TOC,
    /// then the blocks.
    pub fn build(blocks: &[(u16, Vec<u8>)], text_mode: u8) -> Vec<u8> {
        let n = blocks.len() + 1;
        let mut hdr = vec![0u8; 0xC + 6 * n];
        hdr[9] = text_mode;
        hdr[0xA..0xC].copy_from_slice(&(n as u16).to_be_bytes());
        let mut off = hdr.len() as u32;
        let mut entries = vec![(0u32, 0u16)];
        for (t, b) in blocks {
            entries.push((off, *t));
            off += b.len() as u32;
        }
        for (i, (o, t)) in entries.iter().enumerate() {
            hdr[0xC + 6 * i..0x10 + 6 * i].copy_from_slice(&o.to_be_bytes());
            hdr[0x10 + 6 * i..0x12 + 6 * i].copy_from_slice(&t.to_be_bytes());
        }
        let mut out = vec![0u8; 6];
        out.extend(seal(hdr));
        for (_, b) in blocks {
            out.extend(b);
        }
        let len = (out.len() - 2) as u32;
        out[2..6].copy_from_slice(&len.to_be_bytes());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_field_rules() {
        let data = b"  AB  \0Z\xb0\xa1x";
        let c = Container { data, toc: vec![] };
        assert_eq!(text(&c, 0, 6, 10, true, true, 1), b"AB");
        assert_eq!(text(&c, 0, 6, 10, false, false, 1), b"  AB  ");
        assert_eq!(text(&c, 0, 6, 3, false, true, 1), b"AB");
        // EUC-JP "亜" (b0 a1) -> Shift-JIS 88 9f
        assert_eq!(text(&c, 8, 3, 10, false, false, 0), b"\x88\x9fx");
        assert_eq!(text(&c, 8, 3, 10, false, false, 1), b"\xb0\xa1x");
        // a first byte that is not an EUC-JP kanji row (half-width kana prefix 8e): the
        // original's signed arithmetic gives 'w' + 0xaf (seen in its output)
        let kana = Container { data: b"\x8e\xb1", toc: vec![] };
        assert_eq!(text(&kana, 0, 2, 10, false, false, 0), b"w\xaf");
        // other modes drop the 2-byte character and end the string there
        assert_eq!(text(&c, 8, 3, 10, false, false, 3), b"");
    }

    #[test]
    fn bcd() {
        assert_eq!(bcd2(0x0102), (1, 2));
        assert_eq!(bcd2(0x1F2F), (10, 20));
    }

    #[test]
    fn checksum_and_find() {
        let blk = testutil::seal(vec![0, 0, 0, 0, 0, 0, 0, 1, 7, 9]);
        let file = testutil::build(&[(1, blk)], 1);
        let c = Container::new(&file);
        let (p, len) = c.required(1).unwrap();
        assert_eq!(len, 8);
        assert_eq!(c.u8(p + 8), 7);
        let mut bad = file.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(Container::new(&bad).required(1).unwrap_err().message.contains("checksum"));
        assert!(Container::new(&file).required(2).is_err());
    }
}
