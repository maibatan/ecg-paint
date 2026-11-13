//! Field formatters shared by the .txt and the two images (0x40c9d0, 0x40d1a0…0x40e3c0,
//! 0x40cb60, 0x40ceb0, 0x40cc30, 0x40de80). All return Shift-JIS bytes.

use crate::filter::lowpass_label;
use crate::model::{lead_name, DateTime, Record, RecordType};
use crate::settings::atoi;
use crate::strings::load;

/// Language and the -H option, the context every formatter needs.
#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    pub japanese: bool,
}

impl Ctx {
    pub fn s(&self, id: u32) -> Vec<u8> {
        load(id, self.japanese)
    }
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

pub fn sex(c: Ctx, r: &Record) -> Vec<u8> {
    let id = match r.patient.sex {
        1 => 300,
        2 => 301,
        _ => 302,
    };
    cat(&[&c.s(135), &c.s(id)])
}

pub fn age(c: Ctx, r: &Record) -> Vec<u8> {
    let p = &r.patient;
    let mut s = cat(&[&c.s(136), &p.age1]);
    match p.age_code {
        code @ 0..=4 => s.extend(cat(&[b" ", &c.s(400 + code)])),
        code @ 0x10..=0x13 => {
            let k = code - 0x10;
            s.extend(cat(&[b" ", &c.s(400 + k), b" ", &p.age2, &c.s(401 + k)]));
        }
        _ => {}
    }
    s
}

fn measure(int: &[u8], frac: &[u8], unit: &str) -> Vec<u8> {
    let mut s = if int.is_empty() { b"* ".to_vec() } else { int.to_vec() };
    if !frac.is_empty() {
        s.push(b'.');
        s.extend(frac);
    }
    s.extend(unit.as_bytes());
    s
}

pub fn height(r: &Record) -> Vec<u8> {
    let p = &r.patient;
    measure(&p.height.0, &p.height.1, if p.imperial { "inch" } else { "cm" })
}

pub fn weight(r: &Record) -> Vec<u8> {
    let p = &r.patient;
    measure(&p.weight.0, &p.weight.1, if p.imperial { "pound" } else { "kg" })
}

/// "%3.1f" of atoi(s) * 0.1: always n/10 with one decimal.
fn tenths(s: &[u8]) -> Vec<u8> {
    let n = atoi(&String::from_utf8_lossy(s)) as i64;
    let sign = if n < 0 { "-" } else { "" };
    format!("{sign}{}.{}", n.abs() / 10, n.abs() % 10).into_bytes()
}

pub fn blood_pressure(r: &Record) -> Vec<u8> {
    let p = &r.patient;
    let (mut s1, mut s2) = (p.bp.0.clone(), p.bp.1.clone());
    if p.bp_kpa {
        s1 = tenths(&s1);
        s2 = tenths(&s2);
    }
    let star = |v: Vec<u8>| if v.is_empty() { b"*".to_vec() } else { v };
    cat(&[&star(s1), b"/", &star(s2), if p.bp_kpa { b" kpa" } else { b" mmHg" }])
}

/// Medication 1 or 2 (0x40d200).
pub fn medication(c: Ctx, r: &Record, which: u8) -> Vec<u8> {
    let (code, text) = if which == 1 { &r.patient.med1 } else { &r.patient.med2 };
    let mut s = text.clone();
    if (0..=9).contains(code) {
        s = c.s(500 + *code as u32);
    }
    if s.is_empty() {
        s = c.s(500);
    }
    s
}

/// 0x40dc00: "2004/01/13 AM 9:30" (no seconds); year 0 → "".
pub fn date(d: &DateTime) -> Vec<u8> {
    if d.year == 0 {
        return Vec::new();
    }
    let (ampm, h) = if d.hour < 12 { ("AM", d.hour) } else { ("PM", d.hour - 12) };
    format!("{:04}/{:02}/{:02} {} {}:{:02}", d.year, d.month, d.day, ampm, h, d.minute).into_bytes()
}

/// 0x40c9d0: device name and three version pairs.
pub fn device_line(r: &Record) -> Vec<u8> {
    let d = &r.device;
    let mut s = d.name.clone();
    s.extend(
        format!(
            " {:02}-{:02} {:02}-{:02} {:02}-{:02}",
            d.ver1.0, d.ver1.1, d.ver2.0, d.ver2.1, d.ver3.0, d.ver3.1
        )
        .bytes(),
    );
    s
}

/// Exam type line: "Rest", or "after EXER M'SS"" + the Master test part (0x40d390).
pub fn exam(c: Ctx, r: &Record) -> Vec<u8> {
    match r.rtype {
        RecordType::Rest => c.s(100),
        RecordType::AfterExercise => {
            let x = &r.exercise;
            let mut s = c.s(175);
            s.extend(format!(" {}'{:02}\"", x.after.0 as u32 * 60 + x.after.1 as u32, x.after.2).bytes());
            if x.master {
                let kind = match x.kind {
                    0 => "S",
                    1 => "D",
                    2 => "T",
                    3 => "1/2S",
                    4 => "1/4S",
                    _ => "*",
                };
                let bad = |v: u16| v == 0xFFFF || v == 0x8000;
                let t = if bad(x.b) {
                    format!("( */ *[{kind}])")
                } else if bad(x.a) {
                    format!("( */ {}[{kind}])", x.b)
                } else {
                    format!("( {}/ {}[{kind}])", x.a, x.b)
                };
                s.extend(t.bytes());
            }
            s
        }
    }
}

/// "Minnesota (01-02) " (header bytes printed as 10·hi + lo, no F handling).
pub fn minnesota_head(c: Ctx, r: &Record) -> Vec<u8> {
    let h = r.analysis.minnesota_head;
    let bcd = |b: u8| (b >> 4) as u32 * 10 + (b & 0xF) as u32;
    let mut s = c.s(157);
    s.extend(format!(" ({:02}-{:02}) ", bcd(h[0]), bcd(h[1])).bytes());
    s
}

/// One Minnesota code (0x40cb60): " 1-1" or " 1-1-2".
pub fn minnesota_code(e: [u8; 2]) -> Vec<u8> {
    let (p0, p1) = (e[0], e[1]);
    let nib = |v: u8, mul: u32| if v == 0xF { 0 } else { v as u32 * mul };
    let a = nib(p0 >> 4, 10) + nib(p0 & 0xF, 1);
    let b = nib(p1 >> 4, 1);
    if p1 & 0xF == 0xF {
        format!(" {a}-{b}").into_bytes()
    } else {
        format!(" {a}-{b}-{}", p1 & 0xF).into_bytes()
    }
}

/// Statement code text (0x40ceb0): hex nibbles up to the first F.
pub fn statement_code(code: [u8; 3]) -> Vec<u8> {
    let mut s = String::new();
    for b in code {
        if b >> 4 == 0xF {
            break;
        }
        s.push_str(&format!("{:X}", b >> 4));
        if b & 0xF == 0xF {
            break;
        }
        s.push_str(&format!("{:X}", b & 0xF));
    }
    s.into_bytes()
}

fn valid(v: i16, lo: i16, hi: i16) -> Option<String> {
    (lo < v && v < hi).then(|| v.to_string())
}

/// printf "%4.2f" of the original's CRT: the double is first rounded to 17 significant
/// digits, then rounded half-up at 2 decimals. Only used for 0 ≤ x < 1e5.
pub fn fixed2(x: f64) -> String {
    let s = format!("{:.16e}", x);
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: Vec<u8> = mant.bytes().filter(|c| c.is_ascii_digit()).map(|c| c - b'0').collect();
    // value = 0.d1d2…d17 × 10^(exp+1); scale to hundredths
    let point = exp + 1; // digits before the decimal point
    let keep = (point + 2).max(0) as usize; // digits up to the hundredths
    let mut int: u64 = 0;
    for i in 0..keep {
        int = int * 10 + *digits.get(i).unwrap_or(&0) as u64;
    }
    if (point + 2) < 0 {
        int = 0;
    }
    let next = if point + 2 >= 0 { digits.get(keep).copied().unwrap_or(0) } else { 0 };
    if next >= 5 {
        int += 1;
    }
    format!("{}.{:02}", int / 100, int % 100)
}

pub fn heart_rate(r: &Record) -> Vec<u8> {
    format!("{} bpm", valid(r.meas.hr, 0, 10000).unwrap_or("***".into())).into_bytes()
}

pub fn interval(v: i16) -> Vec<u8> {
    format!("{}  ms", valid(v, 0, 10000).unwrap_or("***".into())).into_bytes()
}

pub fn qt_qtc(r: &Record) -> Vec<u8> {
    let qt = valid(r.meas.x7[2], 0, 10000).unwrap_or("***".into());
    match valid(r.meas.x7[3], 0, 10000) {
        Some(qtc) => format!("{qt}/{qtc}  ms").into_bytes(),
        None => b"***  ms".to_vec(),
    }
}

pub fn axes(r: &Record) -> Vec<u8> {
    let ax = |v: i16| (-361 < v && v < 361).then(|| v.to_string());
    let x = &r.meas.x7;
    let p = ax(x[4]).unwrap_or("***".into());
    let q = ax(x[5]).map(|v| format!(" {v}")).unwrap_or(" ***".into());
    let t = ax(x[6]).map(|v| format!(" {v}")).unwrap_or(" ***".into());
    format!("{p}/{q}/{t} deg").into_bytes()
}

fn rv5_sv1_values(r: &Record) -> (i32, i32) {
    let m = &r.meas;
    (m.m_a4.max(m.m_104) as i32, m.m_cc.max(m.m_12c) as i32)
}

pub fn rv5_sv1(r: &Record) -> Vec<u8> {
    let (rv5, sv1) = rv5_sv1_values(r);
    let ok = |v: i32| (0..=0x7FFF).contains(&v);
    if !ok(sv1) {
        return b"***  mV".to_vec();
    }
    let a = if ok(rv5) { fixed2(rv5 as f64 * 0.001) } else { "***".into() };
    format!("{a}/{}  mV", fixed2(sv1 as f64 * 0.001)).into_bytes()
}

pub fn rv5_plus_sv1(r: &Record) -> Vec<u8> {
    let (rv5, sv1) = rv5_sv1_values(r);
    let t = rv5 + sv1;
    let v = if (0..=0x7FFF).contains(&t) { fixed2(t as f64 * 0.001) } else { "***".into() };
    format!("{v}  mV").into_bytes()
}

/// (label id, value) of the seven measurement lines, in output order.
pub fn measurement_lines(r: &Record) -> Vec<(u32, Vec<u8>)> {
    vec![
        (151, heart_rate(r)),
        (152, interval(r.meas.x7[0])),
        (153, interval(r.meas.x7[1])),
        (154, qt_qtc(r)),
        (155, axes(r)),
        (156, rv5_sv1(r)),
        (161, rv5_plus_sv1(r)),
    ]
}

/// Low-pass index actually used for the waves: -H first, else the record (0x405770).
pub fn lowpass_index(r: &Record, h_option: Option<i32>) -> u32 {
    crate::filter::h_option_index(h_option).unwrap_or(r.filters.lowpass)
}

/// 0x40cfe0: "Filter:(H50 d) 35Hz".
pub fn filter_label(r: &Record, h_option: Option<i32>) -> Vec<u8> {
    let hum = match r.filters.hum {
        0 => "H50",
        1 => "H60",
        2 => "Hum OFF",
        _ => "UNKNOWN",
    };
    let drift = match r.filters.drift {
        1 => "d",
        2 => "D",
        _ => "",
    };
    format!("Filter:({hum} {drift}){}", lowpass_label(lowpass_index(r, h_option))).into_bytes()
}

/// "Rhythm[II] 10mm/mV 25mm/s  " + filter label.
pub fn rhythm_label(r: &Record, japanese: bool, lead: Option<usize>, h_option: Option<i32>) -> Vec<u8> {
    let name = lead.map(|l| lead_name(l, japanese)).unwrap_or_default();
    cat(&[b"Rhythm[", &name, b"] 10mm/mV 25mm/s  ", &filter_label(r, h_option)])
}

// ----------------------------------------------------------------------------- statements

/// EN line breaker 0x40ce30: length of the next line (≤ 47 unless no break char).
fn en47(s: &[u8]) -> usize {
    let len = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    if len <= 47 {
        return len;
    }
    let mut k: isize = 46;
    while k >= 0 && !matches!(s[k as usize], 0 | b' ' | b',') {
        k -= 1;
    }
    if k > 0 {
        k as usize + 1
    } else {
        len
    }
}

/// JP line breaker 0x40cd50 (DBCS aware, line-start punctuation kept on the line).
fn jp47(s: &[u8]) -> usize {
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    let mut i = 0;
    while i < 47 && at(i) != 0 {
        if at(i) > 0x80 {
            if i + 2 > 47 {
                break;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    let c = at(i);
    const KINSOKU: [[u8; 2]; 13] = [
        [0x81, 0x41],
        [0x81, 0x42],
        [0x81, 0x43],
        [0x81, 0x44],
        [0x81, 0x6a],
        [0x81, 0x6c],
        [0x81, 0x6e],
        [0x81, 0x70],
        [0x81, 0x72],
        [0x81, 0x74],
        [0x81, 0x76],
        [0x81, 0x78],
        [0x81, 0x7a],
    ];
    if c > 0x80 && KINSOKU.contains(&[c, at(i + 1)]) {
        i += 2;
    } else if c != 0 && c <= 0x80 && b"],.)>".contains(&c) {
        i += 1;
    }
    i.min(s.len()).max(1)
}

/// 0x40cc30: split a statement into lines of at most 47 bytes.
pub fn wrap(src: &[u8], english: bool) -> Vec<Vec<u8>> {
    let end = src.iter().position(|&c| c == 0).unwrap_or(src.len());
    let src = &src[..end];
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < src.len() {
        let n = if english { en47(&src[pos..]) } else { jp47(&src[pos..]) }.max(1);
        let n = n.min(src.len() - pos);
        out.push(src[pos..pos + n].to_vec());
        pos += n;
    }
    out
}

/// The statement block lines ("%c%-5s %-s", continuation "       %-s"), in output order.
///
/// The last statement is always printed last, with the first code; the others carry the code
/// of the statement after them (the original's indexing, kept as it is). When the statements
/// take more than 12 lines the two outputs differ:
/// - the `.txt` (0x408f75) prints all of them (12 slots);
/// - the `_AVE` page (`page` = true, 0x40a7eb) has room for 11 lines: the last statement plus
///   as many of the first ones as fit, then one line "Ohters  " (sic) with the codes of the
///   statements left out, comma separated.
pub fn statement_lines(r: &Record, english: bool, page: bool) -> Vec<Vec<u8>> {
    let a = &r.analysis;
    let text = |i: usize| a.lines.get(i).cloned().unwrap_or_default();
    let code_at = |i: usize| statement_code(a.codes.get(i).map(|c| c.1).unwrap_or([0xFF; 3]));
    let (mut n, mut total) = (0usize, 0usize);
    while n < 12 {
        let k = wrap(&text(n), english).len();
        if k == 0 {
            break;
        }
        total += k;
        n += 1;
    }
    let cnt = if total <= 12 {
        n
    } else if !page {
        12
    } else {
        // lines of the last statement + statements 0, 1, ... while they stay within 11
        let lines_of = |i: usize| wrap(&text(i), english).len();
        let mut used = lines_of(n - 1) + lines_of(0);
        let mut cnt = 1;
        while used <= 11 && cnt < 12 {
            cnt += 1;
            used += lines_of(cnt - 1);
        }
        cnt
    };
    let mut out = Vec::new();
    for i in 0..cnt {
        let last = i + 1 == cnt;
        let t = if last { n.saturating_sub(1) } else { i };
        let code = code_at(if last { 0 } else { i + 1 });
        for (j, line) in wrap(&text(t), english).into_iter().enumerate() {
            let mut l = Vec::new();
            if j == 0 {
                l.push(line[0]);
                l.extend(&code);
                l.extend(std::iter::repeat(b' ').take(5usize.saturating_sub(code.len())));
                l.push(b' ');
                l.extend(&line[1..]);
            } else {
                l.extend(b"       ");
                l.extend(&line);
            }
            out.push(l);
        }
    }
    if page && cnt < n {
        let codes: Vec<Vec<u8>> = (cnt..n).map(code_at).collect();
        out.push([&b"Ohters  "[..], &codes.join(&b","[..])].concat());
    }
    out
}

// ----------------------------------------------------------------------------- comment

/// 0x40e460: insert a space into runs of more than 48 single-byte characters (no line breaks).
fn break_long_runs(s: &mut Vec<u8>) {
    let len0 = s.len();
    let (mut col, mut p) = (0usize, 0usize);
    for i in 0..len0 {
        let c = s.get(p).copied().unwrap_or(0);
        if c == 0 {
            break;
        }
        if c > 0x80 {
            p += 2;
            col = 0;
            continue;
        }
        let nxt = s.get(p + 1).copied().unwrap_or(0);
        p += 1;
        if nxt == b' ' {
            col = 0;
            continue;
        }
        col += 1;
        if col > 48 {
            let at = i.min(s.len());
            s.insert(at, b' ');
            col = 0;
        }
    }
}

/// 0x40de80: the comment text and the "edited" flag.
pub fn comment(r: &Record, japanese: bool) -> (Vec<u8>, bool) {
    let a = &r.analysis;
    let edited = a.edit_date.year != 0;
    let mut s = if edited {
        if !a.long_text.is_empty() {
            a.long_text.clone()
        } else {
            a.short_text.clone()
        }
    } else if japanese {
        a.recommendation.clone()
    } else {
        Vec::new()
    };
    break_long_runs(&mut s);
    (s, edited)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printf_rounding() {
        assert_eq!(fixed2(1600.0 * 0.001), "1.60");
        assert_eq!(fixed2(900.0 * 0.001), "0.90");
        assert_eq!(fixed2(2500.0 * 0.001), "2.50");
        assert_eq!(fixed2(105.0 * 0.001), "0.11"); // 0.10499999999999999 → 17 digits → 0.105 → 0.11
        assert_eq!(fixed2(0.0), "0.00");
        assert_eq!(fixed2(32767.0 * 0.001), "32.77");
        assert_eq!(fixed2(4.0 * 0.001), "0.00");
        assert_eq!(fixed2(5.0 * 0.001), "0.01");
    }

    #[test]
    fn small_formatters() {
        assert_eq!(minnesota_code([0xF1, 0x1F]), b" 1-1");
        assert_eq!(minnesota_code([0x12, 0x34]), b" 12-3-4");
        assert_eq!(statement_code([0x40, 0x48, 0xFF]), b"4048");
        assert_eq!(statement_code([0x10, 0x00, 0xFF]), b"1000");
        let d = DateTime { year: 2004, month: 1, day: 13, hour: 12, minute: 5, second: 0 };
        assert_eq!(date(&d), b"2004/01/13 PM 0:05");
        assert_eq!(tenths(b"-5"), b"-0.5");
        assert_eq!(tenths(b""), b"0.0");
    }

    /// 12 statements of two lines each: the .txt prints all 24 lines, the page only what fits
    /// in 11 lines (statements 0..3 and the last one) plus the codes of the rest.
    #[test]
    fn statements_that_do_not_fit() {
        use crate::model::*;
        let mut analysis = Analysis::default();
        for i in 0..12u8 {
            let mut line = vec![b' '];
            line.extend(format!("statement {i:02} aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj").bytes());
            analysis.lines.push(line);
            analysis.codes[i as usize] = (1, [0x10 + i, 0x00, 0xFF]);
        }
        let r = Record {
            rtype: RecordType::Rest,
            acquired: DateTime::default(),
            device: Device::default(),
            patient: Patient::default(),
            exercise: Exercise::default(),
            filters: Filters::default(),
            meas: Measurements::default(),
            analysis,
            waves: Waves::default(),
        };
        let txt = statement_lines(&r, true, false);
        assert_eq!(txt.len(), 24);
        assert!(txt[0].starts_with(b" 1100  statement 00") && txt[22].starts_with(b" 1000  statement 11"));
        let page = statement_lines(&r, true, true);
        assert_eq!(page.len(), 11);
        assert!(page[6].starts_with(b" 1400  statement 03") && page[8].starts_with(b" 1000  statement 11"));
        assert_eq!(page[10], b"Ohters  1500,1600,1700,1800,1900,1A00,1B00".to_vec());
        // 12 lines or fewer: both the same, no list
        let mut few = r.clone();
        few.analysis.lines.truncate(6);
        assert_eq!(statement_lines(&few, true, true), statement_lines(&few, true, false));
        assert_eq!(statement_lines(&few, true, true).len(), 12);
    }

    #[test]
    fn wrapping() {
        let s = b"aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk";
        let w = wrap(s, true);
        assert_eq!(w[0], b"aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii ".to_vec());
        assert_eq!(w[1], b"jjjj kkkk".to_vec());
        let mut long = vec![b'x'; 60];
        break_long_runs(&mut long);
        assert_eq!(long.len(), 61);
        assert_eq!(long[48], b' ');
    }
}
