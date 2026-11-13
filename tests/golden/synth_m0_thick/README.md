# Golden case `synth_m0_thick`

The `synth_m0` input (`../synth_m0/synth_rest_m0.ecg`) and INI with `IMGDENSITY=1` added, which
makes the original draw the grid with a 2 px pen. Outputs from EcgPaint.exe running natively on
Windows 11 (2026-11-10). This case exists because GDI's wide pen is not a simple thick line: a
grid dot `(x,y)→(x+1,y)` becomes three pixels in an L shape, and a vertical frame line is the
columns `x-1` and `x`. The left frame line looks 3 px wide here only because the grid dots that
start on it fill the next column on every row; at 1280×1024 they leave gaps (case
`synth_m0_thick_1280`). Wine 9 renders wide pens differently again, so a Wine output must not
be used as the reference here. `src/render/mod.rs` (`wide_axis_line`) has the measured rule.

`prims_*.txt` come from the Wine GDI relay trace of the same run (the drawing calls are the same
on both systems; only their rasterisation differs).
