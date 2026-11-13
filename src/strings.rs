//! The string resources EcgPaint loads (LoadString(conv+0x20 + id): 0 = Japanese,
//! 10000 = English). Japanese texts are Shift-JIS (cp932) bytes.

/// (id, English, Japanese cp932)
const TABLE: &[(u32, &str, &[u8])] = &[
    (100, "Rest", b"\x88\xc0\x90\xc3\x8e\x9e"),
    (130, "Acq. Date:", b"\x8c\x9f\x8d\xb8\x93\xfa\x81\x46"),
    (131, "Edit Date:", b"\x95\xd2\x8f\x57\x93\xfa\x81\x46"),
    (132, "Doctor:", b"\x88\xe3\x8e\x74\x96\xbc\x81\x46"),
    (133, "ID:", b"ID:"),
    (134, "Name:", b"\x96\xbc\x91\x4f:"),
    (135, "SEX:", b"\x90\xab\x95\xca\x81\x46"),
    (136, "AGE:", b"\x94\x4e\x97\xdf\x81\x46"),
    (
        140,
        "Need overread by doctor.",
        b"\x8e\xa9\x93\xae\x89\xf0\x90\xcd\x82\xcc\x82\xbd\x82\xdf\x88\xe3\x8e\x74\x82\xc9\x82\xe6\x82\xe9\x8a\x6d\x94\x46\x82\xaa\x95\x4b\x97\x76\x82\xc5\x82\xb7.",
    ),
    (150, "Medication:", b"\x93\x8a\x96\xf2\x8f\xee\x95\xf1\x81\x46"),
    (151, "Heart Rate", b"\x90\x53\x94\x8f\x90\x94"),
    (152, "PR Int.", b"PR\x8a\xd4\x8a\x75"),
    (153, "QRS Int.", b"QRS\x95\x9d"),
    (154, "QT/QTc Int.", b"QT/QTc\x8a\xd4\x8a\x75"),
    (155, "P/QRS/T Axis", b"P/QRS/T\x8e\xb2"),
    (156, "RV5/SV1 Value", b"RV5/SV1\x92\x6c"),
    (157, "Minnesota", b"\xd0\xc8\xbf\xc0\xba\xb0\xc4\xde"),
    (159, "Req.Doctor", b"\x88\xcb\x97\x8a\x88\xe3\x8e\x74\x96\xbc\x81\x46"),
    (160, "Tech.", b"\x8b\x5a\x8e\x74\x96\xbc\x81\x46"),
    (161, "RV5+SV1 Value", b"RV5+SV1\x92\x6c"),
    (175, "after EXER", b"\x95\x89\x89\xd7\x8c\xe3"),
    (300, "Male", b"\x92\x6a"),
    (301, "Female", b"\x8f\x97"),
    (302, "Unknown", b"\x95\x73\x96\xbe"),
    (400, "years", b"\x8d\xcb"),
    (401, "months", b"\x83\x96\x8c\x8e"),
    (402, "weeks", b"\x8f\x54\x8a\xd4"),
    (403, "days", b"\x93\xfa"),
    (404, "hours", b"\x8e\x9e\x8a\xd4"),
    (500, "None", b"None"),
    (501, "Digitalis", b"\x83\x57\x83\x4d\x83\x5e\x83\x8a\x83\x58"),
    (502, "Bata blocker", b"\x83\x78\x81\x5b\x83\x5e\x8e\xd5\x92\x66\x8d\xdc"),
    (503, "Guinidine/Norpace", b"\x83\x4c\x83\x6a\x83\x57\x83\x93"),
    (504, "Diuretic", b"\x97\x98\x94\x41\x8d\xdc"),
    (505, "calcium antagonist", b"\x83\x4a\x83\x8b\x83\x56\x83\x45\x83\x80\x9d\x68\x8d\x52\x8d\xdc"),
    (
        506,
        "Proc/Lido/Tocainide",
        b"\x83\x8a\x83\x68\x83\x4a\x83\x43\x83\x93\x81\x5e\x83\x76\x83\x8d\x83\x4a\x83\x43\x83\x93",
    ),
    (507, "Other antiarrhythmic", b"\x8d\x42\x95\x73\x90\xae\x96\xac\x8d\xdc"),
    (508, "Psychotropic", b"\x90\xb8\x90\x5f\x88\xc0\x92\xe8\x8d\xdc"),
    (509, "Unknown", b"\x95\x73\x96\xbe"),
];

/// LoadString: the resource text as bytes (empty when the id has no string).
pub fn load(id: u32, japanese: bool) -> Vec<u8> {
    TABLE
        .iter()
        .find(|e| e.0 == id)
        .map(|e| if japanese { e.2.to_vec() } else { e.1.as_bytes().to_vec() })
        .unwrap_or_default()
}
