//! Block handlers (parse 0x410c50 and the per-tag readers), in the original's order.

use super::{bcd2, text, Container, ERR_FLAG, ERR_FMT, ERR_MISSING, MAXBUF};
use crate::decomp::{decomp12lead, Cmp, Fid, Meas};
use crate::error::{Error, Result};
use crate::model::*;

/// Wave capacity of R (0x42d9f0).
pub const CAP: usize = 12000;

pub fn parse_record(c: &Container) -> Result<Record> {
    if c.bytes(3, 12) == c.bytes(0x18, 12) {
        return Err(Error::at(3, format!("bytes 3..14 equal bytes 0x18..0x23: not an ECG file ({ERR_FMT:#x})")));
    }
    let p1 = match c.find(1) {
        Some(p) if p < MAXBUF => p,
        _ => return Err(Error::at(0x10, format!("block 1 missing ({ERR_FMT:#x})"))),
    };
    // block 0: header, text mode
    let p0 = c.find(0).ok_or_else(|| Error::at(0x10, format!("block 0 missing ({ERR_MISSING:#x})")))?;
    if p0 + c.be32(p0 + 2) as usize > MAXBUF {
        return Err(Error::at(p0, format!("block 0 runs past 1 MiB ({ERR_MISSING:#x})")));
    }
    let (p0, _) = c.required(0)?;
    let (flag, mode) = (c.u8(p0 + 8), c.u8(p0 + 9));
    if flag > 1 || mode == 2 {
        return Err(Error::at(p0 + 8, format!("header flags {flag}/{mode} ({ERR_FLAG:#x})")));
    }
    let variant = c.be16(p1 + 0x5A);
    let rtype = match variant {
        0 => RecordType::Rest,
        0xFFFE | 0xFFFF => RecordType::AfterExercise,
        10 => return Err(Error::at(p1 + 0x5A, format!("rhythm record (variant 10) is not converted ({ERR_FMT:#x})"))),
        v => return Err(Error::at(p1 + 0x5A, format!("unknown variant {v:#x} ({ERR_FMT:#x})"))),
    };
    let mut rec = Record {
        rtype,
        acquired: DateTime::default(),
        device: Device::default(),
        patient: Patient::default(),
        exercise: Exercise::default(),
        filters: Filters::default(),
        meas: Measurements::default(),
        analysis: Analysis::default(),
        waves: Waves::default(),
    };
    let t7 = if c.find(7).is_some() { 7 } else { 0xB };
    let mut tags: Vec<u16> = vec![1, 2, 0x103, 6, 0x100];
    if rtype == RecordType::AfterExercise {
        tags.push(3);
    }
    tags.extend([0x104, t7, 0x110, 0x113, 8]);
    let has100 = c.find(0x100).is_some();
    for tag in tags {
        match tag {
            1 => tag1(c, &mut rec, mode)?,
            2 => tag2(c, &mut rec, mode)?,
            3 => tag3(c, &mut rec)?,
            6 if !has100 => tag6(c, &mut rec)?,
            0x100 => tag100(c, &mut rec)?,
            7 | 0xB => tag7(c, &mut rec, tag, mode)?,
            0x110 => tag110(c, &mut rec)?,
            8 => tag8(c, &mut rec)?,
            // 0x103, 0x104, 0x113 only feed fields no output uses; a bad checksum still rejects
            0x103 | 0x104 | 0x113 => {
                c.body(tag)?;
            }
            _ => {}
        }
    }
    Ok(rec)
}

fn date_time(c: &Container, o: usize) -> DateTime {
    let year = c.be16(o);
    if year == 0x8000 {
        return DateTime::default();
    }
    let mut d = DateTime { year, month: c.u8(o + 2), day: c.u8(o + 3), ..Default::default() };
    let (h, mi, s) = (c.u8(o + 4), c.u8(o + 5), c.u8(o + 6));
    if h <= 23 && mi <= 59 && s <= 59 {
        (d.hour, d.minute, d.second) = (h, mi, s);
    }
    d
}

/// Block 1 (0x41cb50): device name and version.
fn tag1(c: &Container, rec: &mut Record, mode: u8) -> Result<()> {
    let (p, _) = c.required(1)?;
    rec.device.name = text(c, p + 0xC, 0x3C, 0x3D, true, false, mode);
    rec.device.ver1 = bcd2(c.be16(p + 0x48));
    Ok(())
}

/// Block 2 (0x4196e0): patient, acquisition date.
fn tag2(c: &Container, rec: &mut Record, mode: u8) -> Result<()> {
    let (p, _) = c.required(2)?;
    let t = |off: usize, n: usize, m: usize, trim: bool, skip: bool| text(c, p + off, n, m, trim, skip, mode);
    let b = |off: usize| c.u8(p + off);
    let pt = &mut rec.patient;
    pt.name = t(0x08, 0x14, 0x15, false, false);
    pt.id = t(0x46, 0x14, 0x14, true, false);
    pt.sex = match b(0x5A) {
        0x31 => 1,
        0x32 => 2,
        _ => 0,
    };
    pt.age_code = match b(0x60) {
        c @ 1..=4 => c as u32,
        0x10 => 0x10,
        0x20 => 0x11,
        0x30 => 0x12,
        0x40 => 0x13,
        _ => 0,
    };
    pt.age1 = t(0x61, 3, 4, false, true);
    pt.age2 = if pt.age_code >= 0x10 { t(0x64, 2, 3, false, true) } else { Vec::new() };
    pt.imperial = b(0x66) == 1;
    let frac = |off: usize| if b(off) == 0x20 { Vec::new() } else { t(off, 1, 2, false, true) };
    pt.height = (t(0x67, 3, 4, false, true), frac(0x6A));
    pt.weight = (t(0x6B, 3, 4, false, true), frac(0x6E));
    pt.bp = (t(0x6F, 3, 4, false, true), t(0x72, 3, 4, false, true));
    pt.bp_kpa = b(0x5B) == 1;
    let med = |x: u8| if (1..=9).contains(&x) { x as i32 } else { -1 };
    pt.med1 = (med(b(0x76)), t(0x77, 0x16, 0x17, false, false));
    pt.med2 = (med(b(0x8E)), t(0x8F, 0x16, 0x17, false, false));
    pt.doctor = t(0xD4, 0x14, 0x15, false, false);
    pt.req_doctor = t(0xE9, 0x14, 0x15, false, false);
    pt.technician = t(0xFE, 0x14, 0x15, false, false);
    rec.acquired = date_time(c, p + 0x174);
    Ok(())
}

/// Block 3 (0x417870): exercise test, record type 2 only.
fn tag3(c: &Container, rec: &mut Record) -> Result<()> {
    let (p, _) = c.required(3)?;
    let x = &mut rec.exercise;
    x.master = c.u8(p + 8) == 0xFE;
    let (m, s) = (c.u8(p + 0xC), c.u8(p + 0xD));
    // stored as a Time (m/60, m%60, s); an invalid second keeps the reset value
    x.after = if s <= 59 { (m / 60, m % 60, s) } else { (0, 0, 0) };
    x.b = c.be16(p + 0xE);
    x.a = c.be16(p + 0x10);
    x.kind = match c.u8(p + 0x12) {
        1 => 1,
        2 => 2,
        0xFF => 3,
        0xFE => 4,
        _ => 0,
    };
    Ok(())
}

/// Block 6 (0x41aa20): compact recording settings (used when there is no block 0x100).
fn tag6(c: &Container, rec: &mut Record) -> Result<()> {
    let (p, _) = c.required(6)?;
    rec.device.ver2 = bcd2(c.be16(p + 0xA));
    let f = c.u8(p + 0xF);
    rec.filters = Filters {
        hum: if f & 0x80 != 0 { 0 } else if f & 0x40 != 0 { 1 } else { 2 },
        drift: (f & 0x20 != 0) as u32,
        lowpass: if f & 0x10 != 0 { 0 } else if f & 0x08 != 0 { 1 } else { 5 },
    };
    Ok(())
}

/// Block 0x100 (0x41bba0): full recording settings.
fn tag100(c: &Container, rec: &mut Record) -> Result<()> {
    let Some((p, _)) = c.body(0x100)? else { return Ok(()) };
    rec.device.ver2 = bcd2(c.be16(p + 0xA));
    rec.filters = Filters {
        hum: match c.be16(p + 0x14) {
            1 => 0,
            2 => 1,
            _ => 2,
        },
        drift: match c.be16(p + 0x16) {
            1 => 1,
            2 => 2,
            _ => 0,
        },
        lowpass: match c.be16(p + 0x2C) {
            v @ 1..=5 => v as u32 - 1,
            _ => 5,
        },
    };
    if c.be16(p + 0x4E) != 1 {
        return Err(Error::at(p + 0x4E, format!("block 0x100: mode word != 1 ({ERR_MISSING:#x})")));
    }
    Ok(())
}

/// Block 7 / 0xB (0x416550): measurements, statement codes, Minnesota, edit date, comments.
fn tag7(c: &Container, rec: &mut Record, tag: u16, mode: u8) -> Result<()> {
    let Some((p, _)) = c.body(tag)? else { return Ok(()) };
    let a = &mut rec.analysis;
    a.edit_date = date_time(c, p + 8);
    rec.meas.hr = c.s16(p + 0x10);
    for k in 0..7 {
        rec.meas.x7[k] = c.s16(p + 0x12 + 2 * k);
    }
    rec.meas.m_a4 = c.s16(p + 0x20);
    rec.meas.m_cc = c.s16(p + 0x22);
    rec.device.ver3 = bcd2(c.be16(p + 0x64));
    for k in 0..12 {
        let o = p + 0x66 + 4 * k;
        a.codes[k] = (c.u8(o), [c.u8(o + 1), c.u8(o + 2), c.u8(o + 3)]);
    }
    a.minnesota_head = [c.u8(p + 0x96), c.u8(p + 0x97)];
    a.minnesota.clear();
    for i in 0..8 {
        let e = p + 0x98 + 6 * i;
        if c.u8(e) == 0 {
            break;
        }
        a.minnesota.push([c.u8(e + 2), c.u8(e + 3)]);
    }
    a.class_code = c.be16(p + 0xC8);
    let short_len = c.u8(p + 0xCA) as usize;
    // copied up to the first NUL or 255 bytes, not limited by the length byte
    a.short_text = text(c, p + 0xCB, 0xFF, 0x100, false, false, mode);
    let mut o = p + 0xCB + short_len;
    if o & 1 != 0 {
        o += 1; // aligned on the absolute address (= file offset parity)
    }
    let n = c.s16(o).min(2000);
    a.long_text = if n > 0 { text(c, o + 2, n as usize, 0x7D1, false, false, mode) } else { Vec::new() };
    Ok(())
}

/// Block 0x110 (0x4183f0): per-lead measurements; replaces the whole M table.
fn tag110(c: &Container, rec: &mut Record) -> Result<()> {
    let Some((p, _)) = c.body(0x110)? else { return Ok(()) };
    let w = |idx: usize| c.s16(p + 8 + 2 * idx);
    let m = &mut rec.meas;
    m.m_a4 = w(6 * 12 + 10);
    m.m_104 = w(10 * 12 + 10);
    m.m_cc = w(8 * 12 + 6);
    m.m_12c = w(12 * 12 + 6);
    m.hr = c.s16(p + 8 + 0x2B8);
    // HR is written again from block 7 when there is one (0x416b90 looks for tag 7 only)
    if let Some(p7) = c.find(7) {
        m.hr = c.s16(p7 + 0x10);
    }
    Ok(())
}

fn fiducials(c: &Container, o: usize) -> Fid {
    Fid {
        w0: c.u8(o + 1) as u16 | (c.u8(o + 2) as u16) << 8,
        w2: c.be16(o + 4),
        p4: c.be16(o + 6),
        p6: c.be16(o + 8),
        p8: c.be16(o + 10),
        pa: c.be16(o + 12),
        pc: c.be16(o + 14),
    }
}

const ABSENT_FID: Fid = Fid { w0: 0xFFFF, w2: 0, p4: 0, p6: 0, p8: 0, pa: 0, pc: 0 };

/// Block 8 (0x41d370): compressed waves, decoded right away (Decomp12lead).
fn tag8(c: &Container, rec: &mut Record) -> Result<()> {
    let (p, _) = c.required(8)?;
    let method = match c.be16(p + 0xE) {
        1 => 1,
        2 => 4,
        3 => 5,
        4 => 16,
        _ => 0,
    };
    let (x0, x1) = (c.be16(p + 0x10), c.be16(p + 0x12));
    let flags = c.u8(p + 0x14);
    let mut q = p + 0x16;
    let f = if flags & 1 != 0 {
        q += 16;
        fiducials(c, q - 16)
    } else {
        ABSENT_FID
    };
    for i in 0..4 {
        if flags & (2 << i) == 0 {
            break;
        }
        q += 0x58;
    }
    if flags & 0x20 != 0 {
        q += 16;
    }
    let mut lists: [Vec<(u16, u8)>; 2] = [Vec::new(), Vec::new()];
    for (k, cap) in [(0usize, 0x100usize), (1, 0x200)] {
        let n = c.be16(q) as usize;
        q += 2;
        for i in 0..n.min(cap) {
            lists[k].push((c.be16(q + 4 * i), c.u8(q + 4 * i + 2)));
        }
        q += 4 * n; // the cursor moves past every entry, even beyond the cap
    }
    let qrs: Vec<(u16, u8)> = lists[0].iter().copied().take_while(|&(pos, _)| pos != 0xFFFF).collect();
    let list2: Vec<u16> = lists[1].iter().map(|&(pos, _)| pos).take_while(|&pos| pos != 0xFFFF).collect();
    let count = c.be16(q + 2);
    let mut r = q + 4;
    let mut chunks = [0usize; 9];
    for ch in chunks.iter_mut() {
        *ch = r;
        let len = c.be16(r) as usize;
        r += len + (len & 1);
    }
    let blk = |off: usize| Some(c.rest(off));
    let cmp = Cmp {
        method,
        count,
        main: blk(chunks[0]),
        leads: [
            blk(chunks[1]),
            blk(chunks[2]),
            blk(chunks[3]),
            blk(chunks[4]),
            blk(chunks[5]),
            blk(chunks[6]),
            blk(chunks[7]),
            blk(chunks[8]),
        ],
    };
    let meas = Meas { x0, x1, f, qrs, list2 };
    // the return code is ignored by EcgPaint (0x41d92a)
    let res = decomp12lead(&cmp, &meas, CAP);
    let n = res.count.min(CAP).min(res.records.len());
    rec.waves.records = res.records[..n].to_vec();
    rec.waves.avg = res.avg.iter().flat_map(|l| l.iter().copied()).collect();
    rec.waves.x0 = x0;
    rec.waves.x1 = x1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{build, seal};
    use super::*;
    use crate::decomp::encode::method0_block;

    fn info_blocks() -> Vec<(u16, Vec<u8>)> {
        let mut b1 = vec![0u8; 0x64];
        b1[0xC..0x48].fill(b' ');
        b1[0xC..0x11].copy_from_slice(b"DEVIC");
        b1[0x48..0x4A].copy_from_slice(&[0x01, 0x02]);
        let mut b2 = vec![0u8; 0x1AE];
        b2[0x46..0x4A].copy_from_slice(b"ID01");
        b2[0x5A] = b'2';
        b2[0x61..0x64].copy_from_slice(b" 54");
        b2[0x174..0x17B].copy_from_slice(&[0x07, 0xD4, 1, 13, 25, 30, 15]); // hour 25: time reset
        let mut b6 = vec![0u8; 0x46];
        b6[0xF] = 0x80 | 0x20 | 0x10;
        vec![(1, seal(b1)), (2, seal(b2)), (6, seal(b6))]
    }

    fn tag8_block(count: u16) -> Vec<u8> {
        let mut b = vec![0u8; 0x16];
        b[0x10..0x14].copy_from_slice(&[0, 50, 2, 38]);
        b.extend([0, 0, 0, 0, 0, 0]); // two empty lists, D+0
        b.extend(count.to_be_bytes());
        let avg: Vec<i16> = (0..4800).map(|i| (i % 600) as i16).collect();
        b.extend(method0_block(&avg));
        for l in 0..8 {
            let v: Vec<i16> = (0..count).map(|i| i as i16 * (l + 1)).collect();
            b.extend(method0_block(&v));
        }
        seal(b)
    }

    #[test]
    fn minimal_file() {
        let mut bl = info_blocks();
        bl.push((8, tag8_block(10)));
        let file = build(&bl, 1);
        let rec = parse_record(&Container::new(&file)).unwrap();
        assert_eq!(rec.rtype, RecordType::Rest);
        assert_eq!(rec.device.name, b"DEVIC");
        assert_eq!(rec.device.ver1, (1, 2));
        assert_eq!(rec.patient.id, b"ID01");
        assert_eq!(rec.patient.sex, 2);
        assert_eq!(rec.patient.age1, b"54");
        assert_eq!((rec.acquired.year, rec.acquired.hour), (2004, 0));
        assert_eq!((rec.filters.hum, rec.filters.drift, rec.filters.lowpass), (0, 1, 0));
        assert_eq!(rec.waves.records.len(), 10);
        assert_eq!(rec.waves.records[3][5], 6); // lead II, sample 3
        assert_eq!(rec.waves.records[3][12], 3); // III = II - I
        assert_eq!((rec.waves.x0, rec.waves.x1), (50, 550));
        assert_eq!(rec.waves.avg[601], 1);
    }

    #[test]
    fn rejects() {
        let bl = info_blocks();
        let file = build(&bl, 1); // no block 8
        assert!(parse_record(&Container::new(&file)).unwrap_err().message.contains("block 0x8 missing"));
        let mut bl = info_blocks();
        bl.push((8, tag8_block(4)));
        let file = build(&bl, 2); // text mode 2
        assert!(parse_record(&Container::new(&file)).is_err());
        assert!(parse_record(&Container::new(&[0u8; 64])).is_err());
        assert!(parse_record(&Container::new(&[])).is_err());
    }
}
