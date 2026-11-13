//! The conversion driver (0x41f710): load → `_AVE` → `_6CH` → `.txt`, file names, output folder.

use crate::analtext::AnalTables;
use crate::encode;
use crate::error::{Error, Result, INPUT, RENDER, SAVE};
use crate::fields;
use crate::filter;
use crate::format::{self, ParseOptions};
use crate::model::Record;
use crate::render::ave::{self, PageInput};
use crate::render::layout::Layout;
use crate::render::text::Font;
use crate::render::{sixch, wave, Canvas, Page};
use crate::settings::{self, Flags, ImageFormat, Ini, Settings};
use crate::txt::{self, TxtOptions};
use std::path::{Path, PathBuf};

pub struct Options {
    pub input: PathBuf,
    pub flags: Flags,
    pub ini: PathBuf,
    /// Output folder; None = %TEMP%/EcgViewer/fileout (must exist, like the original).
    pub out_dir: Option<PathBuf>,
    /// t_ecaps12c.dll for the statement texts (None = next to the INI).
    pub ecaps: Option<PathBuf>,
    /// Extra font for characters the bundled one lacks (Japanese).
    pub font: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub files: Vec<PathBuf>,
}

/// ConvertFileName (sysdef 0x100042d0): `" * . / : ; < > ? \ |` → `@` (second bytes of
/// Shift-JIS characters are left alone).
pub fn convert_file_name(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let b = s[i];
        if ((0x81..=0x9F).contains(&b) || (0xE0..=0xFC).contains(&b)) && i + 1 < s.len() {
            out.extend([b, s[i + 1]]);
            i += 2;
            continue;
        }
        out.push(if b"\"*./:;<>?\\|".contains(&b) { b'@' } else { b });
        i += 1;
    }
    out
}

/// 0x407ff0: (`<ID>_` when the ID is shown and not empty) + recording date-time.
pub fn base_name(rec: &Record, show_id: bool) -> String {
    let mut s = Vec::new();
    if show_id && !rec.patient.id.is_empty() {
        s.extend(&rec.patient.id);
        s.push(b'_');
    }
    let d = &rec.acquired;
    if d.year != 0 {
        s.extend(
            format!("{:04}{:02}{:02}{:02}{:02}{:02}", d.year, d.month, d.day, d.hour, d.minute, d.second).bytes(),
        );
    }
    let name = convert_file_name(&s);
    let (txt, _, _) = encoding_rs::SHIFT_JIS.decode(&name);
    txt.into_owned()
}

/// GetTempPath() + "EcgViewer\fileout". GetTempPath looks at TMP, then TEMP; on Unix the
/// platform convention is TMPDIR, so all three are honoured before falling back to /tmp.
pub fn default_out_dir() -> PathBuf {
    let temp = ["TMP", "TEMP", "TMPDIR"]
        .iter()
        .find_map(|v| std::env::var_os(v).filter(|s| !s.is_empty()))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    temp.join("EcgViewer").join("fileout")
}

fn save(path: &Path, data: &[u8]) -> Result<()> {
    std::fs::write(path, data).map_err(|e| Error::new(SAVE, format!("{}: {e}", path.display())))
}

/// Everything the pages need besides the record.
pub struct Prepared {
    pub filtered: Vec<[i16; 16]>,
    pub shifts: [i32; 12],
    pub centres: [i32; 12],
}

/// The wave object (0x4057e0): low-pass filter, derived leads, baseline values.
pub fn prepare(rec: &Record, s: &Settings) -> Prepared {
    let lo = Layout::for_resolution(s.resolution);
    let idx = fields::lowpass_index(rec, s.h_option);
    let filtered = filter::process(&rec.waves.records, idx);
    let shifts = wave::shifts(&filtered, lo.px);
    let centres = wave::centres(&filtered, lo.px);
    Prepared { filtered, shifts, centres }
}

/// Draw both pages (no files written).
pub fn render_pages(
    rec: &Record,
    s: &Settings,
    prep: &Prepared,
    ini: &Ini,
    font: &Font,
    rhythm_lead: Option<usize>,
) -> (Page, Page) {
    let lo = Layout::for_resolution(s.resolution);
    let edited = rec.analysis.edit_date.year != 0;
    let grid = if s.format == ImageFormat::Jpeg {
        settings::grid_color_jpeg(ini, edited)
    } else if edited {
        [0, 0, 255]
    } else {
        [255, 64, 64]
    };
    let input = PageInput {
        rec,
        filtered: &prep.filtered,
        shifts: prep.shifts,
        centres: prep.centres,
        japanese: s.japanese,
        show_id: s.show_id,
        grid,
        grid_width: s.density as i32,
        rhythm_lead,
        h_option: s.h_option,
    };
    let mut c = Canvas::new(lo.w as usize, lo.h as usize, font);
    ave::draw(&mut c, &lo, &input);
    let ave_page = Page::from(c);
    let mut c = Canvas::new(lo.w as usize, lo.h as usize, font);
    sixch::draw(&mut c, &lo, &input);
    (ave_page, Page::from(c))
}

pub fn run(o: &Options) -> Result<Outcome> {
    let ini = Ini::new(&o.ini);
    let s = settings::load(&ini, &o.flags);
    let data = std::fs::read(&o.input).map_err(|e| Error::new(INPUT, format!("{}: {e}", o.input.display())))?;
    let dll = o.ecaps.clone().unwrap_or_else(|| o.ini.with_file_name("t_ecaps12c.dll"));
    let tables = std::fs::read(&dll).ok().and_then(|b| AnalTables::from_dll(&b).ok());
    let rec = format::parse(&data, ParseOptions { english: !s.japanese }, tables.as_ref())?;
    let prep = prepare(&rec, &s);

    let dir = match &o.out_dir {
        Some(d) => {
            std::fs::create_dir_all(d).map_err(|e| Error::new(SAVE, format!("{}: {e}", d.display())))?;
            d.clone()
        }
        None => default_out_dir(),
    };
    let base = base_name(&rec, s.show_id);
    let mut out = Outcome::default();
    if !s.attach {
        let lo = Layout::for_resolution(s.resolution);
        let extra = match &o.font {
            Some(p) => Some(std::fs::read(p).map_err(|e| Error::new(RENDER, format!("{}: {e}", p.display())))?),
            None => None,
        };
        let font = Font::new(lo.font_h, lo.font_w, extra.as_deref()).map_err(|e| Error::new(RENDER, e))?;
        let lead = settings::rhythm_lead(&ini);
        let (ave_page, six_page) = render_pages(&rec, &s, &prep, &ini, &font, lead);
        for (suffix, page) in [("_AVE", ave_page), ("_6CH", six_page)] {
            let bytes = encode::encode(
                &page.rgb,
                page.w,
                page.h,
                s.format,
                settings::png_level(&ini),
                settings::jpeg_quality(&ini),
            )
            .map_err(|e| Error::new(SAVE, e))?;
            let path = dir.join(format!("{base}{suffix}{}", s.format.ext()));
            save(&path, &bytes)?;
            out.files.push(path);
        }
    }
    if s.text_out {
        let opts = TxtOptions {
            japanese: s.japanese,
            show_id: s.show_id,
            rhythm_lead: settings::rhythm_lead(&ini),
            h_option: s.h_option,
        };
        let path = dir.join(format!("{base}.txt"));
        save(&path, &txt::render(&rec, &opts))?;
        out.files.push(path);
    }
    Ok(out)
}
