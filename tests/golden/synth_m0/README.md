# Golden case `synth_m0`

A synthetic rest recording (variant 0, wave method 0 = uncompressed) built from the format spec
by `research/py/waveblocks_pipeline.py`, converted by the original EcgPaint.exe **natively on
Windows 11** with the `ecgviewer.ini` here and no extra flags (2026-11-10). Nothing here comes
from a patient. The same conversion under Wine 9 gives the same `.txt` and the same pixels
outside the text (Wine substitutes another font for Arial).

| file | origin |
|---|---|
| `synth_rest_m0.ecg` | the input (92,886 bytes) |
| `ecgviewer.ini` | the INI the original ran with (IMGTYPE=2 PNG, IMGSIZE=3 → 1024×768, ID/name hidden) |
| `20040113093015.txt` | the original's text output, compared byte for byte |
| `20040113093015_AVE.png`, `_6CH.png` | the original's images, compared pixel for pixel outside text |
| `prims_AVE.txt`, `prims_6CH.txt` | the original's drawing calls (from the Wine GDI relay trace, compacted by `research/scratch/rust/golden_prims.py`), compared in order |

The images were made by GDI with Arial; ours use Liberation Mono, so pixels inside the text
rectangles are allowed to differ (`tests/golden.rs` reports how many).

`../synth_m0_thick` is the same input and INI plus `IMGDENSITY=1` (2 px grid pen), also from
the native Windows run: Wine draws wide pens differently from GDI, so only the Windows output
is a reference for it.
