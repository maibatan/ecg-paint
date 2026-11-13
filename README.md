# ecg-paint

Rust replacement for Nihon Kohden's **EcgPaint.exe** (the converter HHH-Bridge runs under Wine on
Linux): an ECG file from the electrocardiograph becomes two images (`<base>_AVE`, `<base>_6CH`)
and a `<base>.txt`, with the same arguments, INI keys, output folder and file names as the
original, so Bridge only has to swap the binary. No Wine, no vendor DLLs, no runtime dependencies.

**Status:** checked against the original EcgPaint.exe (run natively on Windows 11 and under
Wine 9) on a synthetic recording (uncompressed waves) in 14 configurations: image sizes 0/1/2,
PNG/BMP/JPEG, ID shown/hidden, thick grid, `-H` filter overrides, `-T`, no INI (write-back),
rhythm lead, text output off. `.txt` byte-identical in all of them, every drawing call identical
to the original's GDI trace, PNG/BMP pixel-identical outside the text (text differs because the
bundled font is Liberation Mono, not Arial; JPEG differs by ~1–3/255 from a different encoder).
A second set of 23 synthetic variants (research/py/mkvariants.py) covers compressed waves
(methods 1/4/5/16, decoded by the vendor DLL inside the original vs our port), statement texts
from `t_ecaps12c.dll`, edited records (blue grid, Edit Date, comment block), post-exercise
records, patient-field variants, block 6 settings and 12000-sample records: all `.txt`
byte-identical and 0 pixels different outside text. Not yet checked against a recording from a
real device.

A further round (2026-11-12) compares **whole images, text included**: on Windows,
`examples/gdi_check.rs` lets GDI draw our text calls with the original's font, after which a
page must equal the original's pixel for pixel. All the cases above pass that way (68 pages),
as do Japanese-mode versions of the variants (40 pages); under Wine with a Japanese locale the
strings the original hands to DrawText equal ours (about 10,900 calls, 4,600 with Japanese
text; the only exceptions are the cases listed below). On top of that, 554 inputs from a random generator (`research/py/mkrandom.py`: random
fields, texts in both encodings, signals, wave methods, INI values and flags, plus every one of
the 322 statement codes and 26 advice codes of `t_ecaps12c.dll` in both languages) were run
through both programs. That round found and fixed four differences: the `_AVE` page lists the
codes of statements that do not fit ("Ohters 1577,15778,…", sic) instead of drawing them over
the next block; text fields end at a zero byte and are trimmed like the original's C buffer,
and EUC-JP bytes outside the kanji rows convert with its signed arithmetic; an empty INI value
counts as a missing key and `IMGTYPE`/`IMGDENSITY` use the first character only; the 2 px grid
pen draws vertical lines 2 px wide (visible at 1280×1024). With those, the second half of the
random inputs (300 cases, all valid input) gives: every `.txt` identical (205 English natively,
48 Japanese under Wine), every INI identical after the run, 535 of 536 pages pixel-identical.

Where the two still differ, the original itself misbehaves and ecg-paint does the sensible thing:
- records shorter than 10 s: the original draws memory past its sample buffer in `_6CH`, and
  crashes under about 3600 samples; ecg-paint draws a flat line and converts them;
- a date whose month, day, hour or minute needs three digits, or a year over 9999: the
  original's fixed-size date strings run into each other ("1899/202100/193"); ecg-paint prints
  the numbers;
- `RhythmLead` that is no lead name: the original draws lead index −1 (other memory);
  ecg-paint draws lead II;
- text mode 2 and `-H-1`: the original crashes; ecg-paint exits with −2;
- wave method 5 on 24 s records: 2 of 4 random cases upset the original (one crash, one page
  with two garbage labels, the one page that differs above); ecg-paint converts them, and all
  shorter method-5 records agree;
- a patient ID with Japanese bytes, when the ID is part of the file name: the original passes
  the bytes to the ANSI file API, so the name depends on the Windows code page; ecg-paint
  writes the name a Japanese Windows shows.

Japanese mode writes the `.txt` in Shift-JIS, as the original does on a Japanese Windows (on
another locale the original writes `?` for every Japanese character). Format spec:
[docs/FORMAT.md](docs/FORMAT.md).

```
ecg-paint [options] FILE

Original flags (EcgPaint; -R -L -D -F -I are overridden by ecgviewer.ini, as in the original):
  -R<n>  resolution 0/1/2        -L<e|j>  language     -D<n>  density (1 = thick grid)
  -F<b|j|p>  image format        -I<0|1>  show ID/name
  -T     text only (.txt, TEXTOUT_ATTACH / DEL_IDNAME_ATTACH)
  -H<n>  force the low-pass filter: 0 OFF, 1 25 Hz, 2 35 Hz, 3 75 Hz, 4 100 Hz, 5 150 Hz
Own options:
  --ini FILE       settings file (default: ecgviewer.ini next to this program)
  --out-dir DIR    output folder, created if missing (default: %TEMP%/EcgViewer/fileout, must exist)
  --ecaps FILE     t_ecaps12c.dll for the statement texts (default: next to the INI)
  --font FILE      extra TrueType font for characters the bundled font lacks (Japanese)
  --json           one-line JSON result on stdout
```

Exit code `0` on success, `-2` (shown as 254 by a shell) on any failure, like the original; a
damaged file gives an error message, never a crash. Flag values are attached (`-R1`, `-Fp`) as
MFC parses them; a Unix path keeps its leading `/` (the original dropped it as an option switch).
The output folder follows `TMP`, `TEMP`, then `TMPDIR` (GetTempPath order), so Bridge's existing
`$TEMP/EcgViewer/fileout` convention works unchanged.

## Build and test

```
cargo build --release              # target/release/ecg-paint (~1.4 MB, static except libc)
cargo test                         # 42 tests: unit, decompression vs the Python reference, CLI,
                                   # mutated-file robustness, and the golden comparison below
cargo run --release --example gdi_check -- DIR...   # Windows only: whole-image comparison with
                                   # the original's outputs, text drawn by GDI (see its header)
```

The golden test (`tests/golden.rs`) converts `tests/golden/synth_m0/synth_rest_m0.ecg` and
compares with what EcgPaint.exe produced from the same file and INI. On a mismatch it writes
`diff_AVE.png` / `diff_6CH.png` into `research/scratch/rust/` when that folder exists.

## Layout

| module | role |
|---|---|
| `format`, `decomp` | big-endian block container, checksums, wave decompression (methods 0/1/4/5/16) |
| `model`, `fields`, `strings` | the record, text fields, EUC-JP/Shift-JIS |
| `filter` | low-pass filters, nonlinear smoother, derived leads (III, aVR, aVL, aVF) |
| `render` | aliased GDI-style drawing: grid, calibration pulses, waves, text (fontdue) |
| `encode` | PNG (interlaced, like the original), JPEG, BMP |
| `txt`, `analtext` | the `.txt` report and the statement texts from `t_ecaps12c.dll` |
| `settings` | `ecgviewer.ini` `[DMSX]` with the original's write-back of missing keys |
| `convert` | the driver: load → `_AVE` → `_6CH` → `.txt` |

`research/` (git-ignored) holds the reverse-engineering notes, Python reference implementation,
traces and the original binaries; nothing from a real patient is kept in this repository.

## Licence

GPL-3.0-or-later. `assets/LiberationMono-Regular.ttf` is under the SIL Open Font License.
