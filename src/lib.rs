//! Rust replacement for Nihon Kohden's EcgPaint.exe: an ECG file from the electrocardiograph
//! becomes two images (`_AVE`, `_6CH`) and a `.txt`, like the original. Spec: docs/FORMAT.md.
//!
//! parse (`format`, `decomp`) → model → wave processing (`filter`) → `render` → `encode`;
//! `txt` and `fields` for the text output, `settings` for ecgviewer.ini, `convert` drives it.

pub mod analtext;
pub mod convert;
pub mod decomp;
pub mod encode;
pub mod error;
pub mod fields;
pub mod filter;
pub mod format;
pub mod model;
pub mod render;
pub mod settings;
pub mod strings;
pub mod txt;
