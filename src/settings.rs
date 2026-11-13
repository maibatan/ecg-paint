//! ecgviewer.ini, section [DMSX], read and written the way sysdef.dll does it
//! (GetPrivateProfileString / WritePrivateProfileString semantics, same keys and defaults).

use std::path::{Path, PathBuf};

pub const SECTION: &str = "DMSX";

/// A private-profile file. Reads come from the file each time (like the API); writes
/// update the file in place and never fail the conversion.
pub struct Ini {
    pub path: PathBuf,
}

impl Ini {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Ini { path: path.into() }
    }

    fn content(&self) -> String {
        std::fs::read(&self.path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
    }

    /// The value as a getter of sysdef.dll sees it through a buffer of `size` bytes: at most
    /// `size - 1` characters, and None when the key is absent **or its value is empty** (the
    /// getters test the returned length, so `KEY=` behaves like a missing key).
    pub fn get_n(&self, section: &str, key: &str, size: usize) -> Option<String> {
        let v: String = self.get(section, key)?.chars().take(size.saturating_sub(1)).collect();
        (!v.is_empty()).then_some(v)
    }

    /// GetPrivateProfileString: value of `key` in `section` (both case-insensitive), trimmed,
    /// one pair of surrounding quotes removed; None when absent.
    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        let text = self.content();
        let mut in_sec = false;
        for line in text.lines() {
            let l = line.trim();
            if l.starts_with('[') {
                in_sec = l.trim_start_matches('[').split(']').next().unwrap_or("").trim().eq_ignore_ascii_case(section);
                continue;
            }
            if !in_sec {
                continue;
            }
            if let Some((k, v)) = l.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    let v = v.trim();
                    let v = if v.len() >= 2
                        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
                    {
                        &v[1..v.len() - 1]
                    } else {
                        v
                    };
                    return Some(v.to_string());
                }
            }
        }
        None
    }

    /// WritePrivateProfileString: replace the key's line, or add it at the end of the
    /// section, or add the section. Errors (read-only folder...) are ignored like the original.
    pub fn set(&self, section: &str, key: &str, value: &str) {
        let text = self.content();
        let nl = if text.is_empty() || text.contains("\r\n") { "\r\n" } else { "\n" };
        let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
        let mut sec_start = None;
        let mut sec_end = lines.len();
        let mut replaced = false;
        for (i, line) in lines.iter_mut().enumerate() {
            let l = line.trim();
            if l.starts_with('[') {
                let name = l.trim_start_matches('[').split(']').next().unwrap_or("").trim();
                if sec_start.is_some() {
                    sec_end = i;
                    break;
                }
                if name.eq_ignore_ascii_case(section) {
                    sec_start = Some(i);
                }
                continue;
            }
            if sec_start.is_some() {
                if let Some((k, _)) = l.split_once('=') {
                    if k.trim().eq_ignore_ascii_case(key) {
                        *line = format!("{key}={value}");
                        replaced = true;
                        break;
                    }
                }
            }
        }
        if !replaced {
            match sec_start {
                Some(s) => {
                    // after the last non-empty line of the section
                    let mut at = sec_end;
                    while at > s + 1 && lines[at - 1].trim().is_empty() {
                        at -= 1;
                    }
                    lines.insert(at, format!("{key}={value}"));
                }
                None => {
                    lines.push(format!("[{section}]"));
                    lines.push(format!("{key}={value}"));
                }
            }
        }
        let mut out = lines.join(nl);
        out.push_str(nl);
        let _ = std::fs::write(&self.path, out);
    }
}

/// C atoi: leading spaces, optional sign, digits; anything else stops.
pub fn atoi(s: &str) -> i32 {
    let s = s.trim_start();
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mut v: i64 = 0;
    for c in digits.bytes() {
        if !c.is_ascii_digit() {
            break;
        }
        v = (v * 10 + (c - b'0') as i64).min(i32::MAX as i64 + 1);
    }
    (if neg { -v } else { v }) as i32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Bmp,
    Jpeg,
    Png,
}

impl ImageFormat {
    pub fn ext(self) -> &'static str {
        match self {
            ImageFormat::Bmp => ".bmp",
            ImageFormat::Jpeg => ".jpg",
            ImageFormat::Png => ".png",
        }
    }
}

/// Everything the conversion takes from the command line and the INI.
#[derive(Clone, Debug)]
pub struct Settings {
    /// 0 → 800×600, 1 → 1024×768, 2 → 1280×1024.
    pub resolution: u32,
    pub japanese: bool,
    /// 1 or 2 (grid pen width).
    pub density: u32,
    pub format: ImageFormat,
    /// Show ID/name values and prefix file names with the ID.
    pub show_id: bool,
    /// -T: text only ("attach" mode).
    pub attach: bool,
    /// -H<n> (raw value; None = not given).
    pub h_option: Option<i32>,
    pub text_out: bool,
}

/// Command-line flags as CCommandLineInfo stores them (defaults of ctor 0x41f080).
#[derive(Clone, Debug)]
pub struct Flags {
    pub resolution: i32,
    pub japanese: bool,
    pub density: u32,
    pub format: i32,
    pub show_id: i32,
    pub attach: bool,
    pub h_option: Option<i32>,
}

impl Default for Flags {
    fn default() -> Self {
        Flags { resolution: 0, japanese: true, density: 1, format: 0, show_id: 1, attach: false, h_option: None }
    }
}

/// Language when LocaleInfo is missing or unknown: GetUserDefaultLCID() == 0x411 → Japanese.
fn system_is_japanese() -> bool {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        .map(|s| s.to_ascii_lowercase().starts_with("ja"))
        .unwrap_or(false)
}

/// InitInstance 0x41f380: the INI overrides -R/-L/-D/-F/-I unconditionally; -T and -H stay.
/// Missing TEXTOUT_* / DEL_IDNAME_* keys are written back as "ON".
pub fn load(ini: &Ini, flags: &Flags) -> Settings {
    let size = ini.get(SECTION, "IMGSIZE").unwrap_or_else(|| "1".into());
    let resolution = match size.bytes().next().map(|c| c as i32 - '0' as i32) {
        Some(0) => 0,
        Some(2) => 2,
        _ => 1,
    };
    let japanese = match ini.get(SECTION, "LocaleInfo") {
        Some(v) if v.eq_ignore_ascii_case("japanese") => true,
        Some(v) if v.eq_ignore_ascii_case("english") => false,
        _ => system_is_japanese(),
    };
    // getImgDensity 0x10003c60 / getImgType 0x10003b80: the first character minus '0', like
    // IMGSIZE (not atoi): an empty value is neither 0 nor 2, so `IMGTYPE=` gives BMP
    let first = |key: &str, default: &str| {
        ini.get(SECTION, key).unwrap_or_else(|| default.into()).bytes().next().map(|c| c as i32 - '0' as i32)
    };
    let density = if first("IMGDENSITY", "0") == Some(1) { 2 } else { 1 };
    let format = match first("IMGTYPE", "1") {
        Some(0) => ImageFormat::Jpeg,
        Some(2) => ImageFormat::Png,
        _ => ImageFormat::Bmp,
    };
    let suffix = if flags.attach { "ATTACH" } else { "IMAGE" };
    // getOutputText 0x100038e0 / getOutputFileSecurity 0x10003830: 4-byte buffer; missing or
    // empty -> "ON" is written back
    let on_key = |key: String| match ini.get_n(SECTION, &key, 4) {
        None => {
            ini.set(SECTION, &key, "ON");
            true
        }
        Some(v) => !v.eq_ignore_ascii_case("OFF"),
    };
    let text_out = on_key(format!("TEXTOUT_{suffix}"));
    let hide_id = on_key(format!("DEL_IDNAME_{suffix}"));
    Settings {
        resolution,
        japanese,
        density,
        format,
        show_id: !hide_id,
        attach: flags.attach,
        h_option: flags.h_option,
        text_out,
    }
}

/// getRhythmLead 0x100011c0: display lead index of `RhythmLead` (missing or empty → "II"
/// written back, unknown → None). The value is read through a 4-byte buffer (3 characters).
pub fn rhythm_lead(ini: &Ini) -> Option<usize> {
    let v = match ini.get_n(SECTION, "RhythmLead", 4) {
        None => {
            ini.set(SECTION, "RhythmLead", "II");
            return Some(1);
        }
        Some(v) => v,
    };
    crate::model::LEAD_NAMES.iter().position(|n| n.eq_ignore_ascii_case(&v))
}

pub fn jpeg_quality(ini: &Ini) -> i32 {
    atoi(&ini.get(SECTION, "JPEG_QUALITY").unwrap_or_else(|| "100".into()).chars().take(3).collect::<String>())
}

pub fn png_level(ini: &Ini) -> i32 {
    atoi(&ini.get(SECTION, "PNG_COMPLEVEL").unwrap_or_else(|| "6".into()).chars().take(3).collect::<String>())
}

/// GRID_COLOR_JPEG / GRID_COLOR_EDIT_JPEG ("%d,%d,%d"), JPEG output only.
pub fn grid_color_jpeg(ini: &Ini, edited: bool) -> [u8; 3] {
    let (key, def) = if edited { ("GRID_COLOR_EDIT_JPEG", [0, 0, 255]) } else { ("GRID_COLOR_JPEG", [255, 64, 64]) };
    let Some(v) = ini.get(SECTION, key) else { return def };
    let parts: Vec<i32> = v.split(',').map(atoi).collect();
    if parts.len() < 3 {
        return def;
    }
    [parts[0] as u8, parts[1] as u8, parts[2] as u8]
}

/// The INI next to the executable, like sysdef.dll (GetModuleFileName + "\ecgviewer.ini").
pub fn default_ini_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default()
        .join("ecgviewer.ini")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str, content: &str) -> Ini {
        let p = std::env::temp_dir().join(format!("ecg-paint-test-{}-{name}.ini", std::process::id()));
        std::fs::write(&p, content).unwrap();
        Ini::new(p)
    }

    #[test]
    fn read_and_write_back() {
        let ini = tmp("rw", "[Other]\r\nIMGTYPE=0\r\n[dmsx]\r\n imgtype = \"2\" \r\nIMGSIZE=3\r\n\r\n[X]\r\n");
        assert_eq!(ini.get(SECTION, "IMGTYPE").as_deref(), Some("2"));
        let s = load(&ini, &Flags::default());
        assert_eq!((s.resolution, s.format, s.show_id, s.text_out), (1, ImageFormat::Png, false, true));
        assert_eq!(rhythm_lead(&ini), Some(1));
        let text = std::fs::read_to_string(&ini.path).unwrap();
        assert_eq!(
            text,
            "[Other]\r\nIMGTYPE=0\r\n[dmsx]\r\n imgtype = \"2\" \r\nIMGSIZE=3\r\nTEXTOUT_IMAGE=ON\r\nDEL_IDNAME_IMAGE=ON\r\nRhythmLead=II\r\n\r\n[X]\r\n"
        );
        ini.set(SECTION, "RhythmLead", "aVR");
        assert_eq!(rhythm_lead(&ini), Some(3));
        ini.set(SECTION, "DEL_IDNAME_IMAGE", "off");
        assert!(load(&ini, &Flags::default()).show_id);
        let _ = std::fs::remove_file(&ini.path);
    }

    #[test]
    fn missing_file() {
        let p = std::env::temp_dir().join(format!("ecg-paint-test-{}-none.ini", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let ini = Ini::new(&p);
        let s = load(&ini, &Flags { attach: true, ..Flags::default() });
        assert_eq!((s.resolution, s.format, s.density), (1, ImageFormat::Bmp, 1));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "[DMSX]\r\nTEXTOUT_ATTACH=ON\r\nDEL_IDNAME_ATTACH=ON\r\n");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn empty_and_odd_values() {
        let ini = tmp(
            "odd",
            "[DMSX]\r\nIMGTYPE=\r\nIMGDENSITY=10\r\nDEL_IDNAME_IMAGE=\r\nTEXTOUT_IMAGE=OFFICE\r\nRhythmLead=\r\n",
        );
        let s = load(&ini, &Flags::default());
        // empty IMGTYPE -> BMP; "10" -> thick grid (first character); empty key -> "ON" written
        // back; only 3 characters of the value are seen ("OFF")
        assert_eq!((s.format, s.density, s.show_id, s.text_out), (ImageFormat::Bmp, 2, false, false));
        assert_eq!(rhythm_lead(&ini), Some(1));
        let text = std::fs::read_to_string(&ini.path).unwrap();
        assert!(text.contains("DEL_IDNAME_IMAGE=ON\r\n") && text.contains("RhythmLead=II\r\n"), "{text}");
        let ini2 = tmp("odd2", "[DMSX]\r\nIMGTYPE=02\r\n");
        assert_eq!(load(&ini2, &Flags::default()).format, ImageFormat::Jpeg);
        let _ = std::fs::remove_file(&ini.path);
        let _ = std::fs::remove_file(&ini2.path);
    }

    #[test]
    fn atoi_rules() {
        assert_eq!(atoi(" 12x"), 12);
        assert_eq!(atoi("-3"), -3);
        assert_eq!(atoi("x"), 0);
    }
}
