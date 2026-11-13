//! Plain data: the record EcgPaint builds in memory from the file (docs/FORMAT.md §4.1.16).
//! Text fields are raw Shift-JIS (cp932) bytes, as the original keeps them.

/// Date + time (Date/Time classes 0x429120). `year == 0` means "absent".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// Record type ([rec+0]): 0 = rest ECG, 0xffff = after exercise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordType {
    Rest,
    AfterExercise,
}

/// "B": device / program versions (rec+0x18).
#[derive(Clone, Debug)]
pub struct Device {
    /// B+0x20, block 1 +0x0c (trimmed right).
    pub name: Vec<u8>,
    /// B+0x08/+0x0c, block 1 +0x48 (BCD pair).
    pub ver1: (u32, u32),
    /// B+0x18/+0x1c, block 6 / 0x100 +0x0a.
    pub ver2: (u32, u32),
    /// B+0x10/+0x14, tag 7 +0x64.
    pub ver3: (u32, u32),
}

impl Default for Device {
    /// The constructor 0x429c60 starts every pair as (0, 1): a file without block 7 prints
    /// "00-01" as its third pair (checked against the original).
    fn default() -> Self {
        Device { name: Vec::new(), ver1: (0, 1), ver2: (0, 1), ver3: (0, 1) }
    }
}

/// "C": patient (rec+0x80), the fields the outputs use.
#[derive(Clone, Debug, Default)]
pub struct Patient {
    pub id: Vec<u8>,
    pub name: Vec<u8>,
    /// 1 male, 2 female, 0 unknown.
    pub sex: u32,
    /// Age code (0..4, 0x10..0x13), first and second number.
    pub age_code: u32,
    pub age1: Vec<u8>,
    pub age2: Vec<u8>,
    pub imperial: bool,
    pub height: (Vec<u8>, Vec<u8>),
    pub weight: (Vec<u8>, Vec<u8>),
    pub bp: (Vec<u8>, Vec<u8>),
    pub bp_kpa: bool,
    /// Medication code (1..9, or -1 = free text) + text.
    pub med1: (i32, Vec<u8>),
    pub med2: (i32, Vec<u8>),
    pub doctor: Vec<u8>,
    pub req_doctor: Vec<u8>,
    pub technician: Vec<u8>,
}

/// "X": exercise test (block 3, record type 2 only).
#[derive(Clone, Debug, Default)]
pub struct Exercise {
    /// Block 3 +0x08 == 0xfe.
    pub master: bool,
    /// Time after exercise as stored (Time class: hour, minute, second).
    pub after: (u8, u8, u8),
    /// X+0x18 / X+0x1c (block 3 +0x0e / +0x10).
    pub b: u16,
    pub a: u16,
    /// 0 S, 1 D, 2 T, 3 1/2S, 4 1/4S.
    pub kind: u32,
}

/// "D": filter settings (rec+0x2ac).
#[derive(Clone, Copy, Debug, Default)]
pub struct Filters {
    /// 0 H50, 1 H60, 2 Hum OFF.
    pub hum: u32,
    /// 0 "", 1 "d", 2 "D".
    pub drift: u32,
    /// Low-pass index: 0 25 Hz, 1 35 Hz, 2 75 Hz, 3 100 Hz, 4 150 Hz, 5 OFF.
    pub lowpass: u32,
}

/// Measurements copied from tags 7/0xB and 0x110 (M and X7).
#[derive(Clone, Debug, Default)]
pub struct Measurements {
    /// M+0x2b8.
    pub hr: i16,
    /// X7+2..+0xe: PR, QRS, QT, QTc, P axis, QRS axis, T axis.
    pub x7: [i16; 7],
    /// M+0xa4, M+0x104 (RV5 candidates) and M+0xcc, M+0x12c (SV1 candidates).
    pub m_a4: i16,
    pub m_104: i16,
    pub m_cc: i16,
    pub m_12c: i16,
}

/// Interpretation data of tag 7/0xB ("W").
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    /// 12 statement codes {flag byte, 3 BCD bytes} (W+0x914+2).
    pub codes: [(u8, [u8; 3]); 12],
    /// Recommendation code word (tag 7 +0xc8).
    pub class_code: u16,
    /// Minnesota header (2 raw BCD bytes) and entries (file bytes e2, e3).
    pub minnesota_head: [u8; 2],
    pub minnesota: Vec<[u8; 2]>,
    /// Edit date (W+0x63d4); year 0 = not edited.
    pub edit_date: DateTime,
    pub short_text: Vec<u8>,
    pub long_text: Vec<u8>,
    /// GetAnalText output: statement lines (flag char + text) and recommendation sentence.
    pub lines: Vec<Vec<u8>>,
    pub recommendation: Vec<u8>,
}

/// Waveforms as the decoder leaves them (before the low-pass filter).
#[derive(Clone, Debug, Default)]
pub struct Waves {
    /// N records of 16 int16 (R+0x14); words 4..11 = I, II, V1..V6.
    pub records: Vec<[i16; 16]>,
    /// Average beat, flat: 12 leads x 600 samples, storage order (W+0x252a + 2).
    pub avg: Vec<i16>,
    /// Average window (MH x0, x1).
    pub x0: u16,
    pub x1: u16,
}

#[derive(Clone, Debug)]
pub struct Record {
    pub rtype: RecordType,
    pub acquired: DateTime,
    pub device: Device,
    pub patient: Patient,
    pub exercise: Exercise,
    pub filters: Filters,
    pub meas: Measurements,
    pub analysis: Analysis,
    pub waves: Waves,
}

/// Display order I, II, III, aVR, aVL, aVF, V1..V6 → storage index (table 0x46f270).
pub const DISP2STORE: [usize; 12] = [0, 1, 8, 9, 10, 11, 2, 3, 4, 5, 6, 7];
/// Storage index → display index (table 0x46f2a0).
pub const STORE2DISP: [usize; 12] = [0, 1, 6, 7, 8, 9, 10, 11, 2, 3, 4, 5];
/// Lead names in display order (English; Japanese I/II/III are Shift-JIS roman numerals).
pub const LEAD_NAMES: [&str; 12] = ["I", "II", "III", "aVR", "aVL", "aVF", "V1", "V2", "V3", "V4", "V5", "V6"];
pub const LEAD_NAMES_JP: [&[u8]; 3] = [b"\x87\x54", b"\x87\x55", b"\x87\x56"];

/// Lead name bytes for a display index (wave+0x208).
pub fn lead_name(display: usize, japanese: bool) -> Vec<u8> {
    if japanese && display < 3 {
        LEAD_NAMES_JP[display].to_vec()
    } else {
        LEAD_NAMES.get(display).map(|s| s.as_bytes().to_vec()).unwrap_or_default()
    }
}
