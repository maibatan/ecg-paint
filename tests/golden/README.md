# Golden cases

Synthetic ECG files (nothing from a patient; built from the format spec by
`research/py/waveblocks_pipeline.py` and `research/py/mkvariants.py`) converted by the original
EcgPaint.exe **natively on Windows 11** (2026-11-10). `tests/golden.rs` converts each input with
the INI of its folder and compares: file names, the `.txt` byte for byte, PNG size/interlacing,
and every pixel outside the text rectangles (text differs because we draw with Liberation Mono,
not Arial). `synth_m0` and `synth_m0_thick` also compare the drawing calls against the original's
GDI trace (`prims_*.txt`, from Wine's relay trace).

| case | input | what it covers |
|---|---|---|
| `synth_m0` | `synth_rest_m0.ecg` (method 0, uncompressed) | the main case: container, blocks 1/2/6/8, filters, derived leads, layout, text fields, names |
| `synth_m0_thick` | same input, `IMGDENSITY=1` | GDI's 2 px grid pen (see its README) |
| `synth_m0_thick_1280` | same input, `IMGDENSITY=1`, `IMGSIZE=2` | the 2 px pen at 4.8 px/mm, where the grid dots no longer hide how wide the frame lines are (2026-11-12) |
| `synth_m4` | `synth_m4.ecg` | wave method 4 (lossy, half-rate + QRS templates) through the vendor DLL vs ours |
| `synth_m5` | `synth_m5.ecg` | wave method 5 (half-rate, Catmull-Rom ×2) |
| `synth_m16` | `synth_m16.ecg` | wave method 16 (in-stream Huffman tables, segments, flag columns) |
| `synth_edited_short` | `synth_edited_short.ecg` | edited record: blue grid, "Edit Date:" line, comment block from the short string |
| `synth_exer` | `synth_exer.ecg` | record type 2 (post-exercise, block 3): "after EXER  2'05\"( 3/ 7[D])" |
| `synth_fields` | `synth_fields.ecg`, `DEL_IDNAME_IMAGE=OFF` | ID/name shown and in the file name, female, kPa blood pressure, age in months, inch/pound, free-text medication, PM time |

The statement-text cases (`stmts`, `edited`) need `t_ecaps12c.dll` and are therefore not kept here;
`research/recheck/` runs them (and 15 more variants) against the original when the vendor files are
present in `research/bin/`.

## Whole images, text included

On Windows, `cargo run --release --example gdi_check -- DIR...` draws our text calls with GDI
itself (the original's font) and compares every pixel of both pages with the original's image;
`research/py/mkrandom.py` makes random inputs for it and `research/recheck/fuzz_compare.py`
compares exit codes, file names, the `.txt` and the INI of such a run. That is how the list for
statements that do not fit ("Ohters"), the text-field rules, the INI rules for empty values and
the wide vertical pen were found and fixed (2026-11-12).
