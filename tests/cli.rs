//! Drop-in behaviour of the binary: arguments, INI write-back, exit codes, bad input.

use std::path::{Path, PathBuf};
use std::process::Command;

const CASE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/synth_m0");

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ecg-paint-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(args: &[&str]) -> (i32, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_ecg-paint")).args(args).output().unwrap();
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

/// −2 as the process sees it (254 on Unix).
fn minus_two() -> i32 {
    if cfg!(windows) {
        -2
    } else {
        254
    }
}

#[test]
fn converts_with_original_flags_and_unix_path() {
    let d = tmp_dir("ok");
    let ini = d.join("ecgviewer.ini");
    std::fs::copy(Path::new(CASE).join("ecgviewer.ini"), &ini).unwrap();
    let input = Path::new(CASE).join("synth_rest_m0.ecg");
    let out = d.join("out");
    let (code, stdout, err) = run(&[
        "--ini",
        ini.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--json",
        "-Le",
        "-R1",
        input.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{err}");
    assert!(stdout.starts_with("{\"ok\":true"), "{stdout}");
    for f in ["20040113093015_AVE.png", "20040113093015_6CH.png", "20040113093015.txt"] {
        assert!(out.join(f).is_file(), "{f}");
    }
    // -T: text only
    let out2 = d.join("out2");
    let (code, _, err) =
        run(&["--ini", ini.to_str().unwrap(), "--out-dir", out2.to_str().unwrap(), "-T", input.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    let names: Vec<_> = std::fs::read_dir(&out2).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["20040113093015.txt"]);
    // -T reads the *_ATTACH keys and writes them back (they are in the golden INI already)
    let text = std::fs::read_to_string(&ini).unwrap();
    assert!(text.contains("TEXTOUT_ATTACH=ON"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn missing_keys_written_back_and_id_in_name() {
    let d = tmp_dir("ini");
    let ini = d.join("ecgviewer.ini");
    std::fs::write(&ini, "[DMSX]\r\nIMGTYPE=0\r\nIMGSIZE=0\r\nDEL_IDNAME_IMAGE=OFF\r\n").unwrap();
    let out = d.join("out");
    let input = Path::new(CASE).join("synth_rest_m0.ecg");
    let (code, _, err) = run(&["--ini", ini.to_str().unwrap(), "--out-dir", out.to_str().unwrap(), input.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    let text = std::fs::read_to_string(&ini).unwrap();
    assert_eq!(
        text,
        "[DMSX]\r\nIMGTYPE=0\r\nIMGSIZE=0\r\nDEL_IDNAME_IMAGE=OFF\r\nTEXTOUT_IMAGE=ON\r\nRhythmLead=II\r\n"
    );
    for f in ["ID0001_20040113093015_AVE.jpg", "ID0001_20040113093015_6CH.jpg", "ID0001_20040113093015.txt"] {
        assert!(out.join(f).is_file(), "{f}");
    }
    let txt = std::fs::read(out.join("ID0001_20040113093015.txt")).unwrap();
    assert!(txt.starts_with(b"ID:ID0001\r\nName:TARO TEST"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn bad_input_exits_minus_two_without_output() {
    let d = tmp_dir("bad");
    let ini = d.join("ecgviewer.ini");
    std::fs::copy(Path::new(CASE).join("ecgviewer.ini"), &ini).unwrap();
    let out = d.join("out");
    let garbage = d.join("garbage.ecg");
    std::fs::write(&garbage, (0..5000u32).map(|i| (i * 7919 % 251) as u8).collect::<Vec<u8>>()).unwrap();
    let missing = d.join("missing.ecg");
    let empty = d.join("empty.ecg");
    std::fs::write(&empty, b"").unwrap();
    for f in [&garbage, &missing, &empty] {
        let (code, stdout, err) =
            run(&["--ini", ini.to_str().unwrap(), "--out-dir", out.to_str().unwrap(), "--json", f.to_str().unwrap()]);
        assert_eq!(code, minus_two(), "{}: {err}", f.display());
        assert!(stdout.starts_with("{\"ok\":false"), "{stdout}");
        assert!(!err.is_empty());
    }
    assert_eq!(std::fs::read_dir(&out).map(|r| r.count()).unwrap_or(0), 0);
    // no file at all
    let (code, _, _) = run(&["--ini", ini.to_str().unwrap()]);
    assert_eq!(code, minus_two());
    let _ = std::fs::remove_dir_all(&d);
}
