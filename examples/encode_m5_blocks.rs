//! Method-5 wave blocks for a synthetic test file (research/py/mkvariants.py): the Python
//! reference has no method-5 encoder, so the synthetic input is encoded here and then decoded by
//! the original EcgPaint.exe and by ecg-paint for comparison.
//!
//! `encode_m5_blocks SIGNAL.bin BLOCKS.bin`
//! SIGNAL.bin: u32 LE count, 8 × count i16 LE (I, II, V1..V6), 8 × 600 i16 LE average beats.
//! BLOCKS.bin: the 9 blocks (main, 8 leads) back to back, each starting with its BE16 length.

use ecg_paint::decomp::encode::encode_m5;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: encode_m5_blocks SIGNAL.bin BLOCKS.bin");
        std::process::exit(2);
    }
    let d = std::fs::read(&args[1]).expect("read signal");
    let count = u32::from_le_bytes(d[0..4].try_into().unwrap()) as usize;
    let mut p = 4;
    let mut read = |n: usize| -> Vec<i16> {
        let v = d[p..p + 2 * n].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        p += 2 * n;
        v
    };
    let leads8: Vec<Vec<i16>> = (0..8).map(|_| read(count)).collect();
    let avg8: Vec<Vec<i16>> = (0..8).map(|_| read(600)).collect();
    // method 5 codes `count` half-rate values per lead and the decoder yields 2·count samples
    let (enc, _) = encode_m5(&avg8, &leads8, count / 2);
    let mut out = enc.main.clone().expect("main block");
    for l in &enc.leads {
        out.extend(l.as_ref().expect("lead block"));
    }
    std::fs::write(&args[2], &out).expect("write blocks");
    println!("method 5: count field {}, {} bytes of blocks", enc.count, out.len());
}
