//! Port of research/py/decomp_test.py: encoder -> decoder round trips on synthetic ECGs, plus
//! method 5 (not covered by the Python tests). Signals come from a local deterministic generator,
//! so the numbers differ from the Python run but the checks are the same.

use ecg_paint::analtext::PeImage;
use ecg_paint::decomp::encode::{self, EncodedCmp, PlanSeg};
use ecg_paint::decomp::{
    decomp12lead, decompress_wave, med_decomp12lead, unpack_be16, DecompResult, Fid, Meas, DEFAULT_TABLE,
    DEFAULT_TABLE_VA,
};

const DLL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/research/bin/t_ecaps12c.dll");
const COUNT: usize = 5000;

// ----------------------------------------------------------------------------- synthetic signals

/// xorshift64* + Box-Muller (deterministic, no dependencies).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn unit(&mut self) -> f64 {
        ((self.next() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
    fn gauss(&mut self, sigma: f64) -> f64 {
        sigma * (-2.0 * self.unit().ln()).sqrt() * (std::f64::consts::TAU * self.unit()).cos()
    }
}

fn gauss(t: f64, mu: f64, sigma: f64) -> f64 {
    (-0.5 * ((t - mu) / sigma).powi(2)).exp()
}

/// A plausible beat (mV) around the R peak at t = 0 s.
fn beat_mv(t: f64) -> f64 {
    0.15 * gauss(t, -0.17, 0.025) - 0.10 * gauss(t, -0.025, 0.008) + 1.10 * gauss(t, 0.0, 0.010)
        - 0.25 * gauss(t, 0.025, 0.010)
        + 0.30 * gauss(t, 0.25, 0.045)
}

const GAIN: [f64; 8] = [0.6, 1.0, -0.5, 0.3, 0.8, 1.2, 1.1, 0.9];

struct Synth {
    leads: Vec<Vec<i16>>,
    avg: Vec<Vec<i16>>,
    rpos: Vec<usize>,
}

fn synth(count: usize, seed: u64) -> Synth {
    let (fs, rr, lsb) = (500.0, 60.0 / 72.0, 400.0);
    let mut rng = Rng(seed | 1);
    let beats: Vec<f64> = (0..(count as f64 / fs / rr) as usize + 2).map(|k| 0.35 + k as f64 * rr).collect();
    let rpos = beats.iter().filter(|&&b| b * fs < count as f64 - 1.0).map(|b| (b * fs).round() as usize).collect();
    let leads = (0..8)
        .map(|l| {
            (0..count)
                .map(|i| {
                    let t = i as f64 / fs;
                    let v: f64 = beats.iter().filter(|&&b| (t - b).abs() < 0.7).map(|&b| beat_mv(t - b)).sum();
                    (v * GAIN[l] * lsb + rng.gauss(1.5)).round() as i16
                })
                .collect()
        })
        .collect();
    let avg = (0..8)
        .map(|l| (0..600).map(|k| (beat_mv((k as f64 - 250.0) / fs) * GAIN[l] * lsb / 2.0).round() as i16).collect())
        .collect();
    Synth { leads, avg, rpos }
}

fn lead_of(res: &DecompResult, l: usize) -> Vec<i16> {
    res.records.iter().take(res.count).map(|r| r[4 + l]).collect()
}

fn s16(x: i32) -> i16 {
    x as i16
}

// ----------------------------------------------------------------------------- tables

#[test]
fn default_table_matches_dll() {
    let Ok(dll) = std::fs::read(DLL) else {
        eprintln!("skipped: {DLL} not found");
        return;
    };
    let img = PeImage::parse(&dll).unwrap();
    for (i, &(code, base, nb)) in DEFAULT_TABLE.iter().enumerate() {
        let e = DEFAULT_TABLE_VA as u64 + 16 * i as u64;
        let s = img.cstr(img.u32(e).unwrap() as u64).unwrap();
        assert_eq!(s, code.as_bytes(), "entry {i}");
        assert_eq!(img.s16(e + 4), Some(base), "entry {i}");
        assert_eq!(img.s16(e + 6), Some(nb as i16), "entry {i}");
    }
}

#[test]
fn bit_vector() {
    // values 0, 1, -1, 2 -> differences 0, 1, -2, 3 -> 01 111 1011 0001 -> 7d 88
    let b = encode::huff_encode(&[0, 1, -1, 2]);
    assert_eq!(b, vec![0x7D, 0x88]);
    assert_eq!(decompress_wave(&[0x7D, 0x88], 4), (vec![0, 1, -1, 2], 0));
    assert_eq!(encode::method1_block(&[0, 1, -1, 2]), vec![0x00, 0x04, 0x7D, 0x88]);
    assert_eq!(encode::method0_block(&[0x1234, -2]), vec![0x00, 0x06, 0x12, 0x34, 0xFF, 0xFE]);
}

// ----------------------------------------------------------------------------- DecompressWave

#[test]
fn decompress_wave_round_trip_and_truncation() {
    let mut rng = Rng(7);
    let mut vals: Vec<i16> = (0..500).map(|_| rng.range(-40, 40) as i16).collect();
    vals.extend([32767, -32768, 0, 12345, -300, 301, 13, -13, 28, -29]);
    let mut acc = 0i64;
    let more: Vec<i16> = (0..300)
        .map(|i| {
            acc += vals[i] as i64;
            (acc.rem_euclid(200)) as i16
        })
        .collect();
    vals.extend(more);
    let src = encode::huff_encode(&vals);
    assert_eq!(decompress_wave(&src, vals.len()), (vals.clone(), 0));

    let (out, st) = decompress_wave(&src[..src.len() / 2], vals.len());
    assert_eq!(st, -1);
    let first_zero = (0..out.len()).find(|&i| out[i..].iter().all(|&v| v == 0)).unwrap_or(out.len());
    assert!(first_zero < vals.len());
    assert_eq!(out[..first_zero / 2], vals[..first_zero / 2]);

    assert_eq!(unpack_be16(&encode::pack_be16(&vals), vals.len()), (vals.clone(), 0));
    let mut bad = encode::pack_be16(&vals);
    bad.push(0);
    assert_eq!(unpack_be16(&bad, vals.len()).1, -2);
    assert_eq!(decompress_wave(&[], 3), (vec![0, 0, 0], -1));
    assert_eq!(decompress_wave(&[0x40], 0), (vec![], -1));
}

// ----------------------------------------------------------------------------- methods 0 / 1

#[test]
fn methods_0_and_1_round_trip() {
    let mut s = synth(COUNT, 1);
    let mut rng = Rng(3);
    s.leads[7][..6].copy_from_slice(&[32767, -32768, 32767, -32768, 0, -1]);
    for k in 100..110 {
        s.avg[3][k] = rng.range(-32768, 32767) as i16;
    }
    let meas = Meas::default();
    let mut last = None;
    for method in [0i16, 1] {
        let enc = encode::encode_m01(method, &s.avg, &s.leads, COUNT);
        let res = decomp12lead(&enc.as_cmp(), &meas, 12000);
        assert_eq!(res.status, 0, "method {method}");
        assert_eq!(res.avg_header, 0xD602u16 as i16);
        for l in 0..8 {
            assert_eq!(lead_of(&res, l), s.leads[l], "method {method} lead {l}");
            assert_eq!(res.avg[l].to_vec(), s.avg[l], "method {method} avg {l}");
        }
        last = Some((enc, res));
    }
    let (enc, res) = last.unwrap();
    // derived leads III, aVR, aVL, aVF
    let r = res.records[10];
    let (i, ii) = (r[4] as i32, r[5] as i32);
    assert_eq!(r[12..16], [s16(ii - i), s16((-(i + ii)) >> 1), s16(i - (ii >> 1)), s16(ii - (i >> 1))]);
    let med = med_decomp12lead(&enc.as_cmp(), &meas);
    assert_eq!(med.status, 0);
    assert_eq!(med.avg[..8], res.avg[..8]);
}

// ----------------------------------------------------------------------------- method 4

/// Template contribution of method 4: decode an all-zero rhythm stream with the same average.
fn m4_template_of(meas: &Meas, avg_main: &[u8], count: usize) -> Vec<Vec<i16>> {
    let zeros8 = vec![vec![0i16; 600]; 8];
    let zl = vec![vec![0i16; count]; 8];
    let mut zero = encode::encode_m4(&zeros8, &zl, count, None).0;
    zero.main = Some(avg_main.to_vec());
    let res = decomp12lead(&zero.as_cmp(), meas, 12000);
    (0..8)
        .map(|l| {
            let c = lead_of(&res, l);
            std::iter::once(c[0]).chain(c[1..].iter().map(|&v| v >> 1)).collect()
        })
        .collect()
}

fn max_err(res: &DecompResult, target: &[Vec<i16>], from: usize) -> i32 {
    (0..8)
        .flat_map(|l| {
            let got = lead_of(res, l);
            (from..got.len()).map(move |i| (got[i] as i32 - target[l][i] as i32).abs()).collect::<Vec<_>>()
        })
        .max()
        .unwrap_or(0)
}

#[test]
fn method_4() {
    let s = synth(COUNT, 1);
    // (a) no QRS list: pure stream, exact against the encoder's model
    let (enc, exp) = encode::encode_m4(&s.avg, &s.leads, COUNT, None);
    let res = decomp12lead(&enc.as_cmp(), &Meas::default(), 12000);
    assert_eq!(res.status, 0);
    assert_eq!(res.avg_header, 0x3842);
    for l in 0..8 {
        assert_eq!(lead_of(&res, l), exp.leads[l], "lead {l}");
        assert_eq!(res.avg[l].to_vec(), exp.avg[l], "avg {l}");
    }
    let err = max_err(&res, &s.leads, 1);
    assert!(err <= 40, "max error {err}");
    let aerr = (0..8).flat_map(|l| (0..600).map(move |k| (l, k))).map(|(l, k)| (res.avg[l][k] as i32 - s.avg[l][k] as i32).abs()).max();
    assert!(aerr.unwrap() <= 2, "average error {aerr:?}");

    // (b) with the QRS list: average beat added around each normal beat (type 0, phase 2)
    let meas = Meas { qrs: s.rpos.iter().map(|&p| (p as u16, 0x10)).collect(), ..Default::default() };
    let tpl = m4_template_of(&meas, enc.main.as_ref().unwrap(), COUNT);
    assert!(tpl.iter().any(|l| l.iter().any(|&v| v != 0)), "template path not taken");
    let (enc2, exp2) = encode::encode_m4(&s.avg, &s.leads, COUNT, Some(&tpl));
    let res2 = decomp12lead(&enc2.as_cmp(), &meas, 12000);
    assert_eq!(res2.status, 0);
    for l in 0..8 {
        assert_eq!(lead_of(&res2, l), exp2.leads[l], "template lead {l}");
    }
    assert!(enc2.size() < enc.size(), "template residual should compress better");
    for &p in &s.rpos {
        assert_eq!(res2.records[p][0] & 0xFF, 1, "QRS flag at {p}");
    }
    let med = med_decomp12lead(&enc2.as_cmp(), &meas);
    assert_eq!(med.avg[..8], res2.avg[..8]);
}

// ----------------------------------------------------------------------------- method 5

#[test]
fn method_5() {
    let n = COUNT / 2;
    let s = synth(2 * n, 9);
    let (enc, exp) = encode::encode_m5(&s.avg, &s.leads, n);
    let meas = Meas { x0: 40, x1: 160, ..Default::default() };
    let res = decomp12lead(&enc.as_cmp(), &meas, 12000);
    assert_eq!(res.status, 0);
    assert_eq!(res.count, 2 * n);
    for l in 0..8 {
        assert_eq!(lead_of(&res, l), exp.leads[l], "lead {l}");
        assert_eq!(res.avg[l].to_vec(), exp.avg[l], "avg {l}");
    }
    let err = max_err(&res, &s.leads, 0);
    assert!(err <= 120, "max error {err}");
    // window rewritten (125 - x0, x1 + 124) then doubled
    assert_eq!((res.meas.x0, res.meas.x1), (170, 568));
    let med = med_decomp12lead(&enc.as_cmp(), &meas);
    assert_eq!(med.status, 0);
    assert_eq!(med.avg[..8], res.avg[..8]);
    assert_eq!(med.meas, res.meas);
    let short = decomp12lead(&enc.as_cmp(), &meas, 2 * n - 1);
    assert_eq!((short.status, short.count), (-1, 0));
}

// ----------------------------------------------------------------------------- method 16

fn rnd_flags(count: usize, nch: usize, rng: &mut Rng, init: u16) -> Vec<u16> {
    const BITS: [u16; 9] = [0x8000, 0x4000, 0x2000, 0x1000, 0x0800, 0x0400, 0x0200, 0x0100, 0x0001];
    let mut w = init;
    (0..count)
        .map(|r| {
            if r > 0 && rng.unit() < 0.01 {
                w ^= BITS[rng.range(0, nch as i64 - 1) as usize];
            }
            w
        })
        .collect()
}

#[test]
fn method_16() {
    let s = synth(COUNT, 1);
    let mut rng = Rng(11);
    let (x0, x1) = (40u16, 559u16);
    let meas = Meas { x0, x1, f: Fid { p4: 150, p8: 230, pa: 275, pc: 470, ..Default::default() }, ..Default::default() };
    let tgt: Vec<Vec<i16>> = s.leads.iter().map(|l| l.iter().map(|&v| (v >> 3) << 3).collect()).collect();
    let avg2: Vec<Vec<i16>> = s
        .avg
        .iter()
        .map(|a| a.iter().enumerate().map(|(k, &v)| if (x0 as usize..=x1 as usize).contains(&k) { (v >> 1) << 1 } else { 0 }).collect())
        .collect();
    let flags = [rnd_flags(COUNT, 9, &mut rng, 0x0006), rnd_flags(COUNT, 8, &mut rng, 0x0041), rnd_flags(COUNT, 8, &mut rng, 0)];
    let (enc, _) = encode::encode_m16(&avg2, &tgt, COUNT, x0, x1, Some(&flags), None).unwrap();
    let res = decomp12lead(&enc.as_cmp(), &meas, 12000);
    assert_eq!(res.status, 0);
    for l in 0..8 {
        assert_eq!(lead_of(&res, l), tgt[l], "rhythm lead {l}");
        assert_eq!(res.avg[l].to_vec(), avg2[l], "avg lead {l}");
    }
    for c in 1..4 {
        let col: Vec<u16> = res.records.iter().map(|r| r[c] as u16).collect();
        assert_eq!(col, flags[c - 1], "flag column {c}");
    }
    let med = med_decomp12lead(&enc.as_cmp(), &meas);
    assert_eq!(med.status, 0);
    assert_eq!(med.avg[..8], res.avg[..8]);

    // half-rate segments (type 1, param 0 and 1) mixed with full-rate ones
    let mut plan: Vec<PlanSeg> = Vec::new();
    let (mut pos, mut toggle) = (0usize, 0u8);
    while COUNT > pos {
        let left = COUNT - pos;
        if left > 400 {
            let n = rng.range(30, 119) as usize;
            let param = toggle & 1;
            plan.push((1, param, n));
            pos += 2 * (n - 4) + 3 + param as usize;
            toggle += 1;
            let n2 = rng.range(20, 79) as usize;
            plan.push((0, 0, n2));
            pos += n2;
        } else {
            plan.push((0, 0, left));
            pos += left;
        }
    }
    let (enc2, exp2) = encode::encode_m16(&avg2, &tgt, COUNT, x0, x1, None, Some(&plan)).unwrap();
    let res2 = decomp12lead(&enc2.as_cmp(), &meas, 12000);
    assert_eq!(res2.status, 0);
    for l in 0..8 {
        assert_eq!(lead_of(&res2, l), exp2.leads[l], "plan lead {l}");
    }
    assert!(max_err(&res2, &tgt, 0) < 200);

    // no average: x0 = x1 = 0 -> empty table in the main block, average stays zero
    let (enc3, _) = encode::encode_m16(&avg2, &tgt, COUNT, 0, 0, None, None).unwrap();
    let res3 = decomp12lead(&enc3.as_cmp(), &Meas::default(), 12000);
    assert_eq!(res3.status, 0);
    for (l, t) in tgt.iter().enumerate() {
        assert_eq!(&lead_of(&res3, l), t);
        assert!(res3.avg[l].iter().all(|&v| v == 0));
    }
}

// ----------------------------------------------------------------------------- error statuses

#[test]
fn error_statuses() {
    let s = synth(COUNT, 1);
    let meas = Meas::default();
    let mut enc: EncodedCmp = encode::encode_m01(1, &s.avg, &s.leads, COUNT);
    enc.method = 3;
    let r = decomp12lead(&enc.as_cmp(), &meas, 12000);
    assert_eq!((r.status, r.count), (-3, 0));

    let enc0 = encode::encode_m01(0, &s.avg, &s.leads, COUNT);
    assert_eq!(decomp12lead(&enc0.as_cmp(), &meas, COUNT - 1).status, -32);
    let (enc4, _) = encode::encode_m4(&s.avg, &s.leads, COUNT, None);
    assert_eq!(decomp12lead(&enc4.as_cmp(), &meas, COUNT - 1).status, -1);

    let mut enc1 = encode::encode_m01(1, &s.avg, &s.leads, COUNT);
    let lead3 = enc1.leads[3].clone().unwrap();
    enc1.leads[3] = Some(encode::with_len(&lead3[2..lead3.len() / 2]));
    let r = decomp12lead(&enc1.as_cmp(), &meas, 12000);
    assert_eq!(r.status, -1);
    assert_eq!(r.records[5][12], 0, "derived leads must not be computed on error");

    let tgt: Vec<Vec<i16>> = s.leads.iter().map(|l| l.iter().map(|&v| (v >> 3) << 3).collect()).collect();
    let (enc16, _) = encode::encode_m16(&s.avg, &tgt, COUNT, 0, 0, None, None).unwrap();
    assert_eq!(decomp12lead(&enc16.as_cmp(), &meas, COUNT - 1).status, -64);
}

// ----------------------------------------------------------------------------- robustness

/// Random damage to valid streams of every method plus random measurement data and capacity:
/// the decoder must return (no panic, no hang) with a record count within bounds.
#[test]
fn damaged_streams_never_panic() {
    let n = 300;
    let s = synth(2 * n, 4);
    let tgt: Vec<Vec<i16>> = s.leads.iter().map(|l| l.iter().map(|&v| (v >> 3) << 3).collect()).collect();
    let bases = [
        encode::encode_m01(0, &s.avg, &s.leads, n),
        encode::encode_m01(1, &s.avg, &s.leads, n),
        encode::encode_m4(&s.avg, &s.leads, n, None).0,
        encode::encode_m5(&s.avg, &s.leads, n).0,
        encode::encode_m16(&s.avg, &tgt, n, 40, 559, None, None).unwrap().0,
    ];
    let mut rng = Rng(0xDEC0DE);
    for it in 0..1500 {
        let mut e = bases[it % bases.len()].clone();
        for _ in 0..rng.range(1, 4) {
            let which = rng.range(0, 8) as usize;
            let slot = if which == 0 { &mut e.main } else { &mut e.leads[which - 1] };
            let Some(b) = slot.as_mut() else { continue };
            match rng.range(0, 4) {
                0 if !b.is_empty() => {
                    for _ in 0..rng.range(1, 30) {
                        let j = rng.range(0, b.len() as i64 - 1) as usize;
                        b[j] ^= 1 << rng.range(0, 7);
                    }
                }
                1 => b.truncate(rng.range(0, b.len() as i64) as usize),
                2 if b.len() >= 2 => {
                    b[0] = rng.range(0, 255) as u8;
                    b[1] = rng.range(0, 255) as u8;
                }
                3 => *slot = None,
                _ => b.iter_mut().for_each(|x| *x = rng.range(0, 255) as u8),
            }
        }
        if rng.unit() < 0.1 {
            e.count = rng.range(0, 65535) as u16;
        }
        let rnd16 = |rng: &mut Rng| rng.range(0, 65535) as u16;
        let meas = Meas {
            x0: if rng.unit() < 0.5 { 40 } else { rnd16(&mut rng) },
            x1: if rng.unit() < 0.5 { 559 } else { rnd16(&mut rng) },
            f: Fid {
                w0: rnd16(&mut rng),
                w2: rnd16(&mut rng),
                p4: if rng.unit() < 0.5 { 150 } else { rnd16(&mut rng) },
                p6: 0,
                p8: if rng.unit() < 0.5 { 230 } else { rnd16(&mut rng) },
                pa: if rng.unit() < 0.5 { 275 } else { rnd16(&mut rng) },
                pc: if rng.unit() < 0.5 { 470 } else { rnd16(&mut rng) },
            },
            qrs: (0..rng.range(0, 300)).map(|_| (rnd16(&mut rng) % 900, rng.range(0, 255) as u8)).collect(),
            list2: (0..rng.range(0, 20)).map(|_| rnd16(&mut rng)).collect(),
        };
        let cap = [12000, n, 2 * n, n - 1, 1, 0][rng.range(0, 5) as usize];
        let r = decomp12lead(&e.as_cmp(), &meas, cap);
        assert!(r.count <= cap.max(1) * 2 && r.records.len() == r.count && r.avg.len() == 12);
        let _ = med_decomp12lead(&e.as_cmp(), &meas);
    }
}
