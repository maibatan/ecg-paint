//! GetAnalText of t_ecaps12c.dll: interpretation statement codes -> text lines + the
//! recommendation sentence, as EcgPaint calls it (statement numbers off, '::' details off).
//!
//! Port of `research/py/decomp_analtext.py` (`render_lines`, `comment_line`) and
//! research/notes/decomp.md section 11. The statement tables are read from the vendor DLL bytes
//! at run time (nothing of the vendor text is embedded here). Output is raw bytes as stored in
//! the DLL: Shift-JIS (cp932) for the Japanese table, ASCII for the English one.

/// u32[322]: 24-bit statement codes (e.g. 0x4048ff = statement "4048").
const CODES_VA: u32 = 0x1000_7004;
const N_CODES: usize = 322;
/// char*[393] sentence tables (entries 0..321 parallel to the codes; 322.. are phrases used
/// only by the detail expansion, not read here).
const JP_VA: u32 = 0x1000_8648;
const EN_VA: u32 = 0x1000_8024;
/// char*[26] recommendation lines for codes 0x900 + i.
const JP_COMMENT_VA: u32 = 0x1001_7c58;
const EN_COMMENT_VA: u32 = 0x1001_7bec;
const N_COMMENT: usize = 26;
/// Longest C string read from the image.
const CSTR_MAX: usize = 4096;

// ----------------------------------------------------------------------------- minimal PE32 reader

/// Minimal PE32 image view: image base + section table, virtual address -> file offset.
pub struct PeImage<'a> {
    data: &'a [u8],
    base: u64,
    /// (virtual address incl. image base, virtual size, raw size, raw file offset)
    secs: Vec<(u64, u64, u64, u64)>,
}

fn le16(d: &[u8], off: usize) -> Option<u64> {
    let b = d.get(off..off.checked_add(2)?)?;
    Some(u16::from_le_bytes([b[0], b[1]]) as u64)
}

fn le32(d: &[u8], off: usize) -> Option<u64> {
    let b = d.get(off..off.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64)
}

/// Where a virtual address lives in the file.
enum Loc {
    File(usize),
    /// Inside a section but beyond its raw data (reads as zeros).
    Zero,
}

impl<'a> PeImage<'a> {
    pub fn parse(data: &'a [u8]) -> Result<PeImage<'a>, String> {
        if data.get(0..2) != Some(b"MZ") {
            return Err("not an MZ executable".into());
        }
        let pe = le32(data, 0x3C).ok_or("truncated DOS header")? as usize;
        if data.get(pe..pe.saturating_add(4)) != Some(b"PE\0\0") {
            return Err("no PE signature".into());
        }
        let nsec = le16(data, pe + 6).ok_or("truncated COFF header")? as usize;
        let optsz = le16(data, pe + 20).ok_or("truncated COFF header")? as usize;
        if le16(data, pe + 24) != Some(0x10B) {
            return Err("not a PE32 image".into());
        }
        let base = le32(data, pe + 24 + 28).ok_or("truncated optional header")?;
        let mut secs = Vec::new();
        let mut off = pe + 24 + optsz;
        for _ in 0..nsec.min(96) {
            let f = |k: usize| le32(data, off + k).ok_or("truncated section table");
            let (vsize, va, rsize, rptr) = (f(8)?, f(12)?, f(16)?, f(20)?);
            secs.push((base + va, vsize, rsize, rptr));
            off += 40;
        }
        Ok(PeImage { data, base, secs })
    }

    pub fn image_base(&self) -> u64 {
        self.base
    }

    fn loc(&self, va: u64) -> Option<Loc> {
        for &(sva, vsize, rsize, rptr) in &self.secs {
            if sva <= va && va < sva + vsize.max(rsize) {
                if va - sva >= rsize {
                    return Some(Loc::Zero);
                }
                return usize::try_from(rptr + (va - sva)).ok().map(Loc::File);
            }
        }
        None
    }

    /// `n` bytes at `va` (zeros for uninitialised data or past the end of the file);
    /// None when `va` is not inside any section.
    pub fn read(&self, va: u64, n: usize) -> Option<Vec<u8>> {
        Some(match self.loc(va)? {
            Loc::Zero => vec![0; n],
            Loc::File(o) => (0..n).map(|i| o.checked_add(i).and_then(|p| self.data.get(p)).copied().unwrap_or(0)).collect(),
        })
    }

    pub fn u32(&self, va: u64) -> Option<u32> {
        let b = self.read(va, 4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn s16(&self, va: u64) -> Option<i16> {
        let b = self.read(va, 2)?;
        Some(i16::from_le_bytes([b[0], b[1]]))
    }

    /// NUL-terminated string at `va` (at most 4096 bytes); None when `va` is not mapped.
    pub fn cstr(&self, va: u64) -> Option<Vec<u8>> {
        match self.loc(va)? {
            Loc::Zero => Some(Vec::new()),
            Loc::File(o) => {
                let rest = self.data.get(o..).unwrap_or(&[]);
                let rest = &rest[..rest.len().min(CSTR_MAX)];
                let end = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
                Some(rest[..end].to_vec())
            }
        }
    }
}

// ----------------------------------------------------------------------------- tables

/// The statement and recommendation tables of t_ecaps12c.dll.
pub struct AnalTables {
    codes: Vec<u32>,
    /// [0] = Japanese, [1] = English; None = NULL pointer.
    sentences: [Vec<Option<Vec<u8>>>; 2],
    comments: [Vec<Option<Vec<u8>>>; 2],
}

fn read_ptr_table(img: &PeImage, va: u32, n: usize) -> Result<Vec<Option<Vec<u8>>>, String> {
    (0..n)
        .map(|i| {
            let slot = va as u64 + 4 * i as u64;
            let p = img.u32(slot).ok_or_else(|| format!("table entry {slot:#x} not in image"))?;
            if p == 0 {
                return Ok(None);
            }
            img.cstr(p as u64).map(Some).ok_or_else(|| format!("string pointer {p:#x} not in image"))
        })
        .collect()
}

impl AnalTables {
    /// Read the tables from the bytes of t_ecaps12c.dll.
    pub fn from_dll(bytes: &[u8]) -> Result<AnalTables, String> {
        let img = PeImage::parse(bytes)?;
        if img.image_base() != 0x1000_0000 {
            return Err(format!("unexpected image base {:#x}", img.image_base()));
        }
        let codes = (0..N_CODES)
            .map(|i| img.u32(CODES_VA as u64 + 4 * i as u64).ok_or_else(|| "code table not in image".to_string()))
            .collect::<Result<Vec<u32>, String>>()?;
        // sanity check that this is the DLL build the addresses were taken from
        if codes[0] != 0x0101FF || codes[N_CODES - 1] != 0x9291FF {
            return Err("statement code table not found (different t_ecaps12c.dll build?)".into());
        }
        Ok(AnalTables {
            codes,
            sentences: [read_ptr_table(&img, JP_VA, N_CODES)?, read_ptr_table(&img, EN_VA, N_CODES)?],
            comments: [read_ptr_table(&img, JP_COMMENT_VA, N_COMMENT)?, read_ptr_table(&img, EN_COMMENT_VA, N_COMMENT)?],
        })
    }

    /// Text of one statement code (without the prefix character): the entry of the LAST matching
    /// code, from column 6 to the last non-space byte, stopping before the first "::".
    /// An unknown code gives nothing (its "%06x" form is shorter than 6 characters).
    fn statement_text(&self, lang: usize, code: u32) -> Vec<u8> {
        let mut s: Option<&[u8]> = None;
        let mut found = false;
        for (i, &c) in self.codes.iter().enumerate() {
            if c == code {
                found = true;
                s = self.sentences[lang][i].as_deref();
            }
        }
        let Some(raw) = s.filter(|_| found) else { return Vec::new() };
        let trimmed = raw.iter().rposition(|&c| c != b' ').map_or(0, |p| p + 1);
        let mut out = Vec::new();
        let mut i = 6;
        while i < trimmed {
            if raw.get(i..i + 2) == Some(b"::") {
                break;
            }
            out.push(raw[i]);
            i += 1;
        }
        out
    }
}

/// Validity rule for the flag byte of a statement entry.
fn flag_valid(b: u8) -> bool {
    if b == 0 || b & 0x80 != 0 {
        return false;
    }
    if b & 0x40 != 0 && b & 0x08 == 0 {
        return false;
    }
    if b & 0x20 != 0 && b & 0x0C == 0 {
        return false;
    }
    if b & 0x10 != 0 && b & 0x0E == 0 {
        return false;
    }
    true
}

fn prefix_char(b: u8) -> u8 {
    if b & 8 != 0 {
        b'#'
    } else if b & 4 != 0 {
        b'+'
    } else {
        b' '
    }
}

/// GetAnalText with EcgPaint's settings.
///
/// * `sel`: table selector (anal+0xa0c / +0x1618): 0 = Japanese, 2 = caller-supplied table (NULL in
///   EcgPaint: the DLL returns -1 without output, so both results are empty), anything else = English.
/// * `codes`: the 12 entries {flag byte, BE24 code} of tag 7 +0x66 (anal+0x916).
/// * `class_code`: the recommendation code word (tag 7 +0xc8, anal+0x978), 0x900..0x919.
///
/// Returns the text lines (one per valid non-zero code, the first 9xxxxx classification moved to
/// the end; up to 12) and the recommendation sentence ("NNNN  " prefix removed, trailing spaces
/// trimmed; empty when the code is outside 0x900..0x919).
pub fn get_anal_text(t: &AnalTables, sel: u8, codes: &[(u8, [u8; 3]); 12], class_code: u16) -> (Vec<Vec<u8>>, Vec<u8>) {
    if sel == 2 {
        return (Vec::new(), Vec::new());
    }
    let lang = if sel == 0 { 0 } else { 1 };
    let valid: Vec<(u8, u32)> = codes
        .iter()
        .map(|&(b, c)| {
            let code = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32;
            (b, if flag_valid(b) { code } else { 0 })
        })
        .collect();
    let first9 = valid.iter().position(|&(_, c)| c != 0 && (c & 0xF0_0000) == 0x90_0000);
    let mut order: Vec<usize> = (0..valid.len()).filter(|&i| valid[i].1 != 0 && Some(i) != first9).collect();
    order.extend(first9);
    let lines = order
        .into_iter()
        .map(|i| {
            let mut line = vec![prefix_char(valid[i].0)];
            line.extend(t.statement_text(lang, valid[i].1));
            line
        })
        .collect();
    (lines, comment_line(t, lang, class_code))
}

/// Recommendation line for code word `word` (0x900 + index).
fn comment_line(t: &AnalTables, lang: usize, word: u16) -> Vec<u8> {
    let idx = word.wrapping_sub(0x900) as usize;
    let Some(Some(s)) = t.comments[lang].get(idx) else { return Vec::new() };
    let body = s.get(6..).unwrap_or(&[]);
    let end = body.iter().rposition(|&c| c != b' ').map_or(0, |p| p + 1);
    body[..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_rule() {
        assert!(!flag_valid(0));
        assert!(!flag_valid(0x80));
        assert!(flag_valid(0x02));
        assert!(flag_valid(0x08));
        assert!(flag_valid(0x0C));
        assert!(!flag_valid(0x40));
        assert!(flag_valid(0x48));
        assert!(!flag_valid(0x22));
        assert!(flag_valid(0x24));
        assert!(!flag_valid(0x11));
        assert!(flag_valid(0x12));
    }

    #[test]
    fn garbage_dll_is_rejected_without_panic() {
        assert!(AnalTables::from_dll(&[]).is_err());
        assert!(AnalTables::from_dll(b"MZ").is_err());
        let mut v = vec![0u8; 0x200];
        v[0] = b'M';
        v[1] = b'Z';
        v[0x3C] = 0x80;
        v[0x80..0x84].copy_from_slice(b"PE\0\0");
        v[0x80 + 6] = 0xFF; // many sections, truncated table
        v[0x80 + 24] = 0x0B;
        v[0x80 + 25] = 0x01;
        assert!(AnalTables::from_dll(&v).is_err());
    }
}
