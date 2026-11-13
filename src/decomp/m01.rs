//! Methods 0 (plain BE16) and 1 (default-table Huffman).

use super::{be16, huff_decode, pyslice, set_record_flags, unpack_be16, Cmp, Machine, Meas};

/// Methods 0/1. Returns the negated error bits (bit 0 = a lead failed, bit 1 = the average
/// failed), or -32 when `count` exceeds the capacity. `avg_only` = MedDecomp12lead.
pub(super) fn m01(m: &mut Machine, cmp: &Cmp, meas: &Meas, huff: bool, avg_only: bool) -> i64 {
    let mut status = 0i64;
    let count = cmp.count as usize;
    if !avg_only && count > m.cap {
        return -32;
    }
    let decode = |data: &[u8], n: usize| if huff { huff_decode(data, n) } else { unpack_be16(data, n) };
    // average: 4800 values (8 leads x 600, lead-major); the DLL would crash on a NULL main block
    if let Some(blk) = cmp.main {
        let ln = be16(blk, 0);
        if ln > 2 {
            m.avg[0] = 0xD602u16 as i16; // only written when the block carries data
            let (vals, st) = decode(pyslice(blk, 2, ln as usize), 4800);
            if st == 0 || huff {
                for (i, &v) in vals.iter().enumerate() {
                    m.aset(1 + i as i64, v as i64);
                }
            }
            if st != 0 {
                status |= 2;
            }
        }
    }
    if avg_only {
        return -status;
    }
    for (l, blk) in cmp.leads.iter().enumerate() {
        let Some(blk) = *blk else { continue };
        let ln = be16(blk, 0);
        let col = 4 + l as i64;
        if ln > 2 {
            let (vals, st) = decode(pyslice(blk, 2, ln as usize), count);
            if st == 0 || huff {
                for (r, &v) in vals.iter().enumerate() {
                    m.rset(16 * r as i64 + col, v as i64);
                }
            }
            if st != 0 {
                status |= 1;
            }
        } else {
            for r in 0..count as i64 {
                m.rset(16 * r + col, 0);
            }
        }
    }
    set_record_flags(m, count, meas);
    for r in 0..count as i64 {
        m.rset(16 * r + 1, 0);
        m.rset(16 * r + 2, 0);
        m.rset(16 * r + 3, 0);
    }
    -status
}
