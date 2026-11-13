//! The wave object's processing (ctor 0x4057e0 → 0x405bf0): integer low-pass filter on the
//! 8 recorded channels (words 4..11), then III/aVR/aVL/aVF recomputed (words 12..15).
//! Integer arithmetic exactly as the original (arithmetic shifts, int16 wrap on output).

const NCH: usize = 8;
/// Threshold constant of the common smoother (globals 0x46f9dc…, all 128).
const K: i32 = 128;

/// -H<n> → filter index (0x405770); None = take the record's setting.
pub fn h_option_index(h: Option<i32>) -> Option<u32> {
    match h? {
        0 => Some(5),
        n @ 1..=5 => Some(n as u32 - 1),
        _ => None,
    }
}

/// Label text of a low-pass index (0x40cfe0).
pub fn lowpass_label(idx: u32) -> &'static str {
    match idx {
        0 => " 25Hz",
        1 => " 35Hz",
        2 => " 75Hz",
        3 => "100Hz",
        4 => "150Hz",
        5 => " OFF",
        _ => "UNKNOWN",
    }
}

#[derive(Default)]
struct State {
    prev: [i32; NCH],
    s100: [i32; NCH],
    h75: [[i32; 2]; NCH],
    ring4: [[i32; 4]; NCH],
    acc4: [i32; NCH],
    s35: [i32; NCH],
    ring8: [[i32; 8]; NCH],
    acc8: [i32; NCH],
    r: usize,
}

fn first_sample(idx: u32, x: &[i16; NCH], st: &mut State) {
    for (i, &v) in x.iter().enumerate() {
        let v = v as i32;
        match idx {
            0 => {
                st.acc8[i] = v << 3;
                st.ring8[i] = [v; 8];
            }
            1 => {
                st.ring4[i] = [v; 4];
                st.acc4[i] = v << 2;
                st.s35[i] = v << 4;
            }
            2 => st.h75[i] = [v, v],
            3 => st.s100[i] = v << 4,
            _ => {}
        }
        st.prev[i] = v << 5;
    }
    st.r = 0;
}

fn next_sample(idx: u32, x: &[i16; NCH], st: &mut State) -> [i16; NCH] {
    let mut tmp = [0i32; NCH];
    for i in 0..NCH {
        let v = x[i] as i32;
        tmp[i] = match idx {
            0 => {
                // 8-point moving sum, x4
                let old = st.ring8[i][st.r];
                st.acc8[i] = st.acc8[i].wrapping_add(v - old);
                st.ring8[i][st.r] = v;
                st.acc8[i].wrapping_mul(4)
            }
            1 => {
                // 4-point moving sum, then t = s + 4·acc, s = t >> 1
                let old = st.ring4[i][st.r];
                st.acc4[i] = st.acc4[i].wrapping_add(v - old);
                st.ring4[i][st.r] = v;
                let t = st.s35[i].wrapping_add(st.acc4[i].wrapping_mul(4));
                st.s35[i] = t >> 1;
                t
            }
            2 => {
                let [x1, x2] = st.h75[i];
                st.h75[i] = [v, x1];
                8 * (v + 2 * x1 + x2)
            }
            3 => {
                let e = 18 * v - (st.s100[i] >> 3);
                let t = st.s100[i].wrapping_add(e);
                st.s100[i] = e;
                t
            }
            _ => v << 5,
        };
    }
    match idx {
        0 => st.r = (st.r + 1) & 7,
        1 => st.r = (st.r + 1) & 3,
        _ => {}
    }
    // common edge-preserving smoother 0x426820 (values x32)
    let mut out = [0i16; NCH];
    for i in 0..NCH {
        let (c, p) = (tmp[i], st.prev[i]);
        let d = c.wrapping_sub(p);
        let y = if d >= 4 * K {
            c - 3 * K
        } else if d <= -4 * K {
            c + 3 * K
        } else {
            c.wrapping_add(p.wrapping_mul(3)) >> 2
        };
        st.prev[i] = y;
        out[i] = (y >> 5) as i16;
    }
    out
}

/// III, aVR, aVL, aVF from I and II (EXE formulas 0x405c47…0x405c82).
pub fn derive(i: i16, ii: i16) -> [i16; 4] {
    let (a, b) = (i as i32, ii as i32);
    [
        (b - a) as i16,
        (-((a + b) >> 1)) as i16,
        (a - ((ii >> 1) as i32)) as i16,
        (b - ((i >> 1) as i32)) as i16,
    ]
}

/// Filter all records with low-pass index `idx` (5 = OFF: plain copy) and recompute the
/// derived leads. Words 0..3 are not written by the original when a filter runs (left 0 here).
pub fn process(records: &[[i16; 16]], idx: u32) -> Vec<[i16; 16]> {
    let mut st = State::default();
    let mut out = Vec::with_capacity(records.len());
    for (n, smp) in records.iter().enumerate() {
        let mut x = [0i16; NCH];
        x.copy_from_slice(&smp[4..12]);
        let mut o = [0i16; 16];
        if idx >= 5 {
            o = *smp;
        } else if n == 0 {
            first_sample(idx, &x, &mut st);
            o[4..12].copy_from_slice(&x);
        } else {
            o[4..12].copy_from_slice(&next_sample(idx, &x, &mut st));
        }
        let d = derive(o[4], o[5]);
        o[12..16].copy_from_slice(&d);
        out.push(o);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_settles() {
        for idx in 0..6 {
            let recs: Vec<[i16; 16]> = (0..40)
                .map(|_| {
                    let mut r = [0i16; 16];
                    r[4] = 1000;
                    r[5] = -500;
                    r[6] = 200;
                    r
                })
                .collect();
            let out = process(&recs, idx);
            assert_eq!((out[39][4], out[39][5]), (1000, -500), "idx {idx}");
            assert_eq!(out[39][12], -1500);
        }
    }

    #[test]
    fn derived_formulas() {
        assert_eq!(derive(3, 0), [-3, -1, 3, -1]);
        assert_eq!(derive(-3, 0), [3, 2, -3, 2]);
    }

    #[test]
    fn h_option() {
        assert_eq!(h_option_index(Some(0)), Some(5));
        assert_eq!(h_option_index(Some(2)), Some(1));
        assert_eq!(h_option_index(Some(9)), None);
        assert_eq!(h_option_index(None), None);
    }
}
