//! Drop-in command line of EcgPaint.exe: `ecg-paint [-R n] [-L e|j] [-D n] [-F b|j|p] [-I 0|1]
//! [-T] [-H n] FILE`, parsed like MFC's ParseCommandLine + ParseParam 0x41f100, plus a few long
//! options of our own. Exit code 0 on success, −2 on any failure (like the original).

use ecg_paint::convert::{self, Options};
use ecg_paint::error::{Error, ARGS};
use ecg_paint::settings::{self, atoi, Flags};
use std::path::PathBuf;

const USAGE: &str = "usage: ecg-paint [options] FILE

Converts a Nihon Kohden ECG file into <base>_AVE, <base>_6CH images and <base>.txt, like EcgPaint.exe.

Original flags (as in EcgPaint; -R -L -D -F -I are overridden by ecgviewer.ini, as in the original):
  -R<n>  resolution 0/1/2     -L<e|j>  language     -D<n>  density (1 = thick grid)
  -F<b|j|p>  image format     -I<0|1>  show ID/name
  -T     text only (.txt, TEXTOUT_ATTACH / DEL_IDNAME_ATTACH)
  -H<n>  force the low-pass filter: 0 OFF, 1 25 Hz, 2 35 Hz, 3 75 Hz, 4 100 Hz, 5 150 Hz
Own options:
  --ini FILE       settings file (default: ecgviewer.ini next to this program)
  --out-dir DIR    output folder, created if missing (default: %TEMP%/EcgViewer/fileout, must exist)
  --ecaps FILE     t_ecaps12c.dll for the statement texts (default: next to the INI)
  --font FILE      extra TrueType font for characters the bundled font lacks (Japanese)
  --json           one-line JSON result on stdout
  -h, --help       this text";

struct Cli {
    opts: Options,
    json: bool,
    input: Option<PathBuf>,
}

fn parse_args(args: Vec<String>) -> Result<Option<Cli>, Error> {
    let mut flags = Flags::default();
    let mut input: Option<PathBuf> = None;
    let (mut ini, mut out_dir, mut ecaps, mut font, mut json) = (None, None, None, None, false);
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| Error::new(ARGS, format!("{name} needs a value")));
        match a.as_str() {
            "-h" | "--help" | "/?" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "--version" => {
                println!("ecg-paint {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--json" => json = true,
            "--ini" => ini = Some(PathBuf::from(value("--ini")?)),
            "--out-dir" => out_dir = Some(PathBuf::from(value("--out-dir")?)),
            "--ecaps" => ecaps = Some(PathBuf::from(value("--ecaps")?)),
            "--font" => font = Some(PathBuf::from(value("--font")?)),
            _ => {
                let first = a.chars().next();
                let rest: String = a.chars().skip(1).collect();
                if matches!(first, Some('-') | Some('/')) && rest.chars().count() < 3 {
                    flag(&mut flags, &rest);
                } else if input.is_none() {
                    // a Unix path keeps its leading '/'; MFC would strip it (and a '-')
                    let name = if first == Some('-') { rest } else { a };
                    input = Some(PathBuf::from(name));
                }
            }
        }
    }
    let ini = ini.unwrap_or_else(settings::default_ini_path);
    let opts = Options { input: input.clone().unwrap_or_default(), flags, ini, out_dir, ecaps, font };
    Ok(Some(Cli { opts, json, input }))
}

/// One short switch (letter + at most one character), case-insensitive.
fn flag(f: &mut Flags, s: &str) {
    let mut ch = s.chars();
    let letter = ch.next().map(|c| c.to_ascii_uppercase());
    let arg: String = ch.collect();
    let a0 = arg.chars().next().map(|c| c.to_ascii_lowercase());
    match letter {
        Some('R') => f.resolution = atoi(&arg),
        Some('L') => match a0 {
            Some('e') => f.japanese = false,
            Some('j') => f.japanese = true,
            _ => {}
        },
        Some('D') => f.density = if atoi(&arg) == 1 { 2 } else { 1 },
        Some('F') => match a0 {
            Some('b') => f.format = 0,
            Some('j') => f.format = 1,
            Some('p') => f.format = 2,
            _ => {}
        },
        Some('I') => f.show_id = atoi(&arg),
        Some('T') => f.attach = true,
        Some('H') => f.h_option = Some(atoi(&arg)),
        _ => eprintln!("ecg-paint: ignoring unknown option -{s}"),
    }
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn main() {
    let args: Vec<String> = std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
    let json = args.iter().any(|a| a == "--json");
    let result = parse_args(args).and_then(|cli| match cli {
        None => Ok(None),
        Some(cli) if cli.input.is_none() => Err(Error::new(ARGS, "no input file (see --help)")),
        Some(cli) => convert::run(&cli.opts).map(|o| Some((o, cli.json))),
    });
    match result {
        Ok(None) => {}
        Ok(Some((out, json))) => {
            if json {
                let files: Vec<String> = out.files.iter().map(|p| json_str(&p.display().to_string())).collect();
                println!("{{\"ok\":true,\"files\":[{}]}}", files.join(","));
            }
        }
        Err(e) => {
            if json {
                println!("{{\"ok\":false,\"stage\":{},\"error\":{}}}", json_str(e.stage), json_str(&e.message));
            }
            eprintln!("ecg-paint: {e}");
            std::process::exit(-2);
        }
    }
}
