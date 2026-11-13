//! The .txt output (0x408630): CRLF lines, Shift-JIS. The separators after "Medication"
//! lines and measurement labels are written with sizeof(literal), so they carry the
//! literal's terminating NUL byte — kept here byte for byte.

use crate::fields::{self, Ctx};
use crate::model::Record;

const NL: &[u8] = b"\r\n";
/// Write(0x47b314 "   ", 4)
const SEP4: &[u8] = b"   \0";
/// Write(0x47b310 "  ", 3)
const SEP3: &[u8] = b"  \0";

/// Inputs of the writer besides the record.
pub struct TxtOptions {
    pub japanese: bool,
    pub show_id: bool,
    /// Display index of RhythmLead (None = unknown name in the INI).
    pub rhythm_lead: Option<usize>,
    pub h_option: Option<i32>,
}

pub fn render(r: &Record, o: &TxtOptions) -> Vec<u8> {
    let c = Ctx { japanese: o.japanese };
    let mut f: Vec<u8> = Vec::new();
    let line = |f: &mut Vec<u8>, parts: &[&[u8]]| {
        for p in parts {
            f.extend_from_slice(p);
        }
        f.extend_from_slice(NL);
    };
    let p = &r.patient;
    let empty: &[u8] = b"";
    line(&mut f, &[&c.s(133), if o.show_id { &p.id } else { empty }]);
    line(&mut f, &[&c.s(134), if o.show_id { &p.name } else { empty }]);
    line(&mut f, &[&fields::sex(c, r)]);
    line(&mut f, &[&fields::age(c, r)]);
    line(&mut f, &[&fields::height(r), b" ", &fields::weight(r), b" ", &fields::blood_pressure(r)]);
    line(&mut f, &[&c.s(150)]);
    line(&mut f, &[SEP4, &fields::medication(c, r, 1)]);
    line(&mut f, &[SEP4, &fields::medication(c, r, 2)]);
    for (id, v) in fields::measurement_lines(r) {
        line(&mut f, &[&c.s(id), SEP3, &v]);
    }
    line(&mut f, &[&c.s(160), SEP3, &p.technician]);
    line(&mut f, &[&c.s(159), SEP3, &p.req_doctor]);
    for l in fields::statement_lines(r, !o.japanese, false) {
        line(&mut f, &[&l]);
    }
    let (comment, edited) = fields::comment(r, o.japanese);
    line(&mut f, &[&comment]);
    if edited {
        let d = fields::date(&r.analysis.edit_date);
        line(&mut f, &[&c.s(131), &d, NL, &c.s(132), &p.doctor]);
    } else {
        line(&mut f, &[&c.s(140), NL, &c.s(132)]);
    }
    line(&mut f, &[&fields::device_line(r)]);
    line(&mut f, &[&fields::exam(c, r)]);
    line(&mut f, &[&fields::minnesota_head(c, r)]);
    for e in &r.analysis.minnesota {
        line(&mut f, &[&fields::minnesota_code(*e)]);
    }
    line(&mut f, &[&fields::date(&r.acquired)]);
    line(&mut f, &[&fields::rhythm_label(r, o.japanese, o.rhythm_lead, o.h_option)]);
    f
}
