//! Golden vectors produced by tests/gen_golden.py from the Python reference (research/py):
//! the Rust decoder, encoders and GetAnalText must give identical results.

use ecg_paint::analtext::{get_anal_text, AnalTables};
use ecg_paint::decomp::encode::{self, EncodedCmp, PlanSeg};
use ecg_paint::decomp::{decomp12lead, med_decomp12lead, Cmp, DecompResult, Fid, Meas};
use std::collections::BTreeMap;

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/decomp");
const DLL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/research/bin/t_ecaps12c.dll");

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> &'a [u8] {
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        s
    }
    fn u8(&mut self) -> u8 {
        self.take(1)[0]
    }
    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.take(2).try_into().unwrap())
    }
    fn i16(&mut self) -> i16 {
        self.u16() as i16
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take(4).try_into().unwrap())
    }
    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.take(8).try_into().unwrap())
    }
    fn str(&mut self) -> String {
        let n = self.u16() as usize;
        String::from_utf8(self.take(n).to_vec()).unwrap()
    }
    fn blk(&mut self) -> Option<Vec<u8>> {
        let n = self.u32();
        (n != 0xFFFF_FFFF).then(|| self.take(n as usize).to_vec())
    }
    fn i16s(&mut self, n: usize) -> Vec<i16> {
        (0..n).map(|_| self.i16()).collect()
    }
    fn meas(&mut self) -> Meas {
        let v: Vec<u16> = (0..9).map(|_| self.u16()).collect();
        let nq = self.u32() as usize;
        let qrs = (0..nq).map(|_| (self.u16(), self.u8())).collect();
        let nl = self.u32() as usize;
        let list2 = (0..nl).map(|_| self.u16()).collect();
        Meas {
            x0: v[0],
            x1: v[1],
            f: Fid { w0: v[2], w2: v[3], p4: v[4], p6: v[5], p8: v[6], pa: v[7], pc: v[8] },
            qrs,
            list2,
        }
    }
    fn magic(&mut self, m: &[u8]) -> usize {
        assert_eq!(self.take(4), m);
        self.u32() as usize
    }
}

fn fnv1a64(data: impl IntoIterator<Item = u8>) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01B3);
    }
    h
}

fn result_bytes(r: &DecompResult) -> Vec<u8> {
    let mut out = Vec::new();
    for rec in &r.records {
        for v in rec {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    for a in &r.avg {
        for v in a {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

fn blocks(r: &mut Rd) -> (Option<Vec<u8>>, [Option<Vec<u8>>; 8]) {
    let main = r.blk();
    let leads: [Option<Vec<u8>>; 8] = std::array::from_fn(|_| r.blk());
    (main, leads)
}

#[test]
fn decoder_matches_python_reference() {
    let data = std::fs::read(format!("{DATA}/golden_decode.bin")).expect("run tests/gen_golden.py");
    let mut r = Rd { b: &data, p: 0 };
    let n = r.magic(b"DPG1");
    let mut fails = Vec::new();
    let mut statuses: BTreeMap<i16, usize> = BTreeMap::new();
    let mut full = 0;
    for _ in 0..n {
        let name = r.str();
        let med = r.u8() == 1;
        let has_full = r.u8() == 1;
        let method = r.i16();
        let count = r.u16();
        let cap = r.u32() as usize;
        let (main, leads) = blocks(&mut r);
        let meas = r.meas();
        let (st, cnt, hdr) = (r.i16(), r.u32() as usize, r.i16());
        let meas_out = r.meas();
        let hash = r.u64();
        let cmp = Cmp {
            method,
            count,
            main: main.as_deref(),
            leads: std::array::from_fn(|i| leads[i].as_deref()),
        };
        let res = if med { med_decomp12lead(&cmp, &meas) } else { decomp12lead(&cmp, &meas, cap) };
        *statuses.entry(res.status).or_default() += 1;
        let rb = result_bytes(&res);
        let mut why = Vec::new();
        if res.status != st {
            why.push(format!("status {} != {}", res.status, st));
        }
        if res.count != cnt {
            why.push(format!("count {} != {}", res.count, cnt));
        }
        if res.avg_header != hdr {
            why.push(format!("avg header {} != {}", res.avg_header, hdr));
        }
        if res.meas != meas_out {
            why.push("meas differs".into());
        }
        if fnv1a64(rb.iter().copied()) != hash {
            why.push("records/avg hash differs".into());
        }
        if has_full {
            full += 1;
            let exp = r.take(cnt * 32 + 7200 * 2);
            if rb.len() != exp.len() {
                why.push("full output length differs".into());
            } else if let Some(i) = (0..rb.len() / 2).find(|&i| rb[2 * i..2 * i + 2] != exp[2 * i..2 * i + 2]) {
                let w = |b: &[u8]| i16::from_le_bytes([b[2 * i], b[2 * i + 1]]);
                let at = if i < cnt * 16 {
                    format!("record {} word {}", i / 16, i % 16)
                } else {
                    let k = i - cnt * 16;
                    format!("avg lead {} sample {}", k / 600, k % 600)
                };
                why.push(format!("first difference at {at}: {} != {}", w(&rb), w(exp)));
            }
        }
        if !why.is_empty() {
            fails.push(format!("{name}: {}", why.join(", ")));
        }
    }
    assert_eq!(r.p, data.len());
    eprintln!("{n} decoder cases ({full} with full output); status histogram {statuses:?}");
    assert!(fails.is_empty(), "{} of {n} cases differ:\n{}", fails.len(), fails.join("\n"));
}

#[test]
fn encoders_match_python_reference() {
    let data = std::fs::read(format!("{DATA}/golden_encode.bin")).expect("run tests/gen_golden.py");
    let mut r = Rd { b: &data, p: 0 };
    let n = r.magic(b"DPS1");
    for _ in 0..n {
        let name = r.str();
        let enc = r.u8();
        let count = r.u16() as usize;
        let avg8: Vec<Vec<i16>> = (0..8).map(|_| r.i16s(600)).collect();
        let nl = r.u32() as usize;
        let leads8: Vec<Vec<i16>> = (0..8).map(|_| r.i16s(nl)).collect();
        let template = (r.u8() == 1).then(|| (0..8).map(|_| r.i16s(count)).collect::<Vec<_>>());
        let (x0, x1) = (r.u16(), r.u16());
        let flags = (r.u8() == 1).then(|| -> [Vec<u16>; 3] { std::array::from_fn(|_| (0..count).map(|_| r.u16()).collect()) });
        let np = r.u32() as usize;
        let plan: Vec<PlanSeg> = (0..np).map(|_| (r.u8(), r.u8(), r.u16() as usize)).collect();
        let (main, leads) = blocks(&mut r);
        let got: EncodedCmp = match enc {
            0 | 1 => encode::encode_m01(enc as i16, &avg8, &leads8, count),
            4 => encode::encode_m4(&avg8, &leads8, count, template.as_deref()).0,
            5 => encode::encode_m5(&avg8, &leads8, count).0,
            16 => {
                let p = (!plan.is_empty()).then_some(plan.as_slice());
                encode::encode_m16(&avg8, &leads8, count, x0, x1, flags.as_ref(), p).unwrap().0
            }
            _ => panic!("unknown encoder {enc}"),
        };
        assert_eq!(got.main, main, "{name}: main block");
        for (l, (g, e)) in got.leads.iter().zip(leads.iter()).enumerate() {
            assert_eq!(g, e, "{name}: lead block {l}");
        }
    }
    assert_eq!(r.p, data.len());
}

#[test]
fn analtext_matches_python_reference() {
    let Ok(dll) = std::fs::read(DLL) else {
        eprintln!("skipped: {DLL} not found");
        return;
    };
    let t = AnalTables::from_dll(&dll).unwrap();
    let data = std::fs::read(format!("{DATA}/golden_analtext.bin")).expect("run tests/gen_golden.py");
    let mut r = Rd { b: &data, p: 0 };
    let n = r.magic(b"DPA1");
    let mut fails = Vec::new();
    for _ in 0..n {
        let name = r.str();
        let sel = r.u8();
        let codes: [(u8, [u8; 3]); 12] = std::array::from_fn(|_| {
            let f = r.u8();
            let c = r.u32();
            (f, [(c >> 16) as u8, (c >> 8) as u8, c as u8])
        });
        let word = r.u16();
        let nlines = r.u32() as usize;
        let hash = r.u64();
        let (lines, com) = get_anal_text(&t, sel, &codes, word);
        let mut blob = Vec::new();
        for l in &lines {
            blob.extend_from_slice(l);
            blob.push(b'\n');
        }
        blob.push(b'|');
        blob.extend_from_slice(&com);
        if lines.len() != nlines || fnv1a64(blob) != hash {
            fails.push(name);
        }
    }
    assert_eq!(r.p, data.len());
    eprintln!("{n} GetAnalText cases");
    assert!(fails.is_empty(), "{} of {n} cases differ: {:?}", fails.len(), fails);
}
