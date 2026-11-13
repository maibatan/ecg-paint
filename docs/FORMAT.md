# EcgPaint.exe — định dạng và hành vi (đặc tả cho bản Rust)

Trạng thái: **bản nháp từ phân tích tĩnh, chưa kiểm chứng trên mẫu nào** (0 file thật, 0 golden; hợp nhất 2026-11-04).
Mỗi khẳng định có bằng chứng: địa chỉ trong `EcgPaint.exe` (base 0x400000, ghi "EXE 0x…" hoặc chỉ "0x…"),
trong `t_ecaps12c.dll` (base 0x10000000, ghi "DLL 0x1000…"), trong `sysdef.dll` ("sysdef 0x1000…"), hoặc trace trong `research/traces/…`.
**(đoán)** = chưa kiểm chứng (thường là ý nghĩa vật lý của một trường, hoặc đơn vị).

Chi tiết và lập luận đầy đủ: `research/NOTES.md` (dòng lệnh, INI, luồng chạy, trace), `research/notes/container.md`
(file chứa, block thông tin, record), `research/notes/waveblocks.md` (block sóng/đo, bộ lọc, dữ liệu được vẽ),
`research/notes/decomp.md` (t_ecaps12c.dll: giải nén, bảng câu kết luận). Mô hình tham chiếu bằng Python: `research/py/`
(`container_parse.py`, `waveblocks_parse.py`, `waveblocks_pipeline.py`, `waveblocks_filters.py`, `decomp_decode.py`,
`decomp_encode.py`, `decomp_analtext.py`). Kế hoạch file tổng hợp: `research/notes/synthetic-plan.md`.

Quy ước: số nhiều byte **trong file** luôn là **big-endian** (swap16 0x411ac0/0x416d40/0x40f350/0x40f380, swap32 0x40f430/0x40f3e0);
trong bộ nhớ (record, struct DLL) là little-endian. "BE16/BE32" = u16/u32 big-endian; "s" = có dấu (bù 2).
"body" (thân block) = con trỏ FIND_BLOCK trả về (mục 4.1.2). `B<tag>+x` = byte x của thân block tag.

Tên đối tượng trong record dùng ở đây (ba ghi chú gốc đặt tên khác nhau — xem Phụ lục A #9):

| Ở đây | Record off | container.md | waveblocks.md | decomp.md |
|---|---|---|---|---|
| B (phiên bản/thiết bị) | +0x18 | B | "đối tượng số/phiên bản" | — |
| C (bệnh nhân) | +0x80 | C | — | — |
| D (bộ lọc) | +0x2ac | D | **F** | — |
| R (sóng nhịp) | +0x2c0 | "wave data" | R | wave |
| F (tuỳ chọn block 0x104) | +0x5e834 | F | **W2** | — |
| W (đo đạc/kết luận/nhịp TB) | +0x5e840 | **G** | W | meas/anal (bản sao trong DLL) |
| H / I (thiết lập, kiểu 1) | +0x654fc / +0x655c0 | H / I | TL2 / TL3 | — |
| X (gắng sức, kiểu 2) | +0x654fc | X | — | — |
| X7 (khoảng/trục tag 7) | W+0x614c | — | **X** | meas+0x4742 |
| M (bảng đo tag 0x110) | W+0x5d6c | — | M | meas+0x475c |
| Fm (mốc nhịp TB, A1 của tag 8) | W+0x173c | — | "F" | F (meas+0) |

## 1. Chương trình làm gì

Máy điện tim Nihon Kohden gửi một file đo qua FTP (user CEX) vào `%TEMP%\ftp` của máy chạy HHH-Bridge; Bridge gọi
`EcgPaint.exe <file>` (Windows) hoặc `wine "<exe>" <file>` (Linux), tham số **không** có ngoặc kép, chờ tối đa 30 s, bỏ qua
exit code, rồi gửi mọi file trong `%TEMP%\EcgViewer\fileout` lên HIS (NOTES.md "Bridge gọi nó thế nào").
EcgPaint đọc file chứa (container) big-endian gồm các block có checksum, dựng một record trong bộ nhớ (bệnh nhân, thiết lập,
số đo, mã câu kết luận), giải nén sóng 8 kênh (I, II, V1..V6) và nhịp trung bình bằng `t_ecaps12c.dll`, lọc thông thấp,
tính lại III/aVR/aVL/aVF, rồi vẽ hai ảnh — `<base>_AVE` (12 nhịp trung bình + một dải nhịp 10 s + văn bản) và `<base>_6CH`
(12 chuyển đạo, 2 cột × 5 s) — và ghi `<base>.txt`. **EcgPaint không tự phân tích**: số đo và mã câu kết luận được chép từ file;
các hàm phân tích trong DLL là stub (NOTES.md "Xử lý"); phần thật sự cần DLL chỉ là giải nén và bảng câu kết luận —
cả hai đã được mô tả đủ để bản Rust không cần DLL (mục 4.2, 5.4).

```
argv → InitInstance 0x41f380 (cờ dòng lệnh, rồi INI ghi đè) → thread → driver 0x41f710
  driver: conv = new 0x40 (0x407780); Init(res, jp, density) 0x407940; Load(file) 0x407c70
    Load: đọc ≤1 MiB (0x411400) → record kiểu 1, parse 0x410c50; lỗi → record kiểu 2, parse lại; lỗi → thất bại
          parse: header + block 1 → biến thể → vòng tag (handler từng block; tag 8 giải nén qua DLL) → GetAnalText (DLL)
          wave object 0x4057e0: lọc thông thấp + dẫn xuất; tính dịch đường đáy (0x405db0, 0x406010)
  nếu không -T: tên _AVE 0x408180 → vẽ 0x409d30 → lưu 0x4084d0; tên _6CH 0x408290 → vẽ 0x40ba10 → lưu
  nếu TEXTOUT: tên .txt 0x4083a0 → ghi 0x408630
  xong → kết quả 0 (0x41f979); lỗi → −2 (0x41f966)
```

## 2. Dòng lệnh

Parser: `CWinApp::ParseCommandLine` 0x466b89 → `ParseParam` 0x41f100 (ký tự đầu `-` hoặc `/` là cờ; chỉ nhận cờ có độ dài < 3,
không phân biệt hoa thường; bảng switch 0x41f2c8/0x41f2a8). Object CCommandLineInfo toàn cục 0x5455b8 (ctor 0x41f080).

| Tuỳ chọn | Đối số | Ý nghĩa | Mặc định | Có tác dụng? | Bằng chứng |
|---|---|---|---|---|---|
| `<file>` | đường dẫn | tham số không phải cờ đầu tiên (hoặc "cờ" dài ≥ 3 ký tự) = file vào; chỉ tham số đầu được dùng | — | có | `m_strFileName` 0x5455cc; 0x41f127, 0x41f288 |
| `-R<n>` | 0/1/2 | độ phân giải ảnh → 0x5455dc | 0 | **không** (INI `IMGSIZE` ghi đè vô điều kiện) | 0x41f15e…; ghi đè 0x41f464…0x41f486 |
| `-L<e\|j>` | e/j | ngôn ngữ → 0x5455e0 (e=0, j=1) | 1 | **không** (INI `LocaleInfo`) | ghi đè 0x41f4a1 |
| `-D<n>` | số | mật độ: 1 → 2, khác → 1 → 0x5455e4 | 1 | **không** (INI `IMGDENSITY`) | ghi đè 0x41f4ae/0x41f4ba |
| `-F<b\|j\|p>` | b/j/p | định dạng ảnh → 0x5455e8 (b=0, j=1, p=2) | 0 | **không** (INI `IMGTYPE`) | ghi đè 0x41f4d2…0x41f4e2 |
| `-I<0\|1>` | 0/1 | hiện ID/tên → 0x5455ec | 1 | **không** (INI `DEL_IDNAME_*`) | ghi đè 0x41f53b |
| `-T` | — | chế độ "attach": **không vẽ ảnh**, chỉ .txt; dùng `TEXTOUT_ATTACH`/`DEL_IDNAME_ATTACH` thay cho `*_IMAGE` | tắt | có | 0x5456f8; driver 0x41f7bc→0x41f8d1; InitInstance 0x41f4ec |
| `-H<n>` | 0..5 | ép bộ lọc thông thấp: 0 OFF, 1 25 Hz, 2 35 Hz, 3 75 Hz, 4 100 Hz, 5 150 Hz; khác → theo file | −1 | có | 0x5456fc; 0x405770 (bảng 0x4057a8) |

- Chuỗi usage 0x41ed80 là code chết (chỉ jmp stub 0x401217 tham chiếu); chạy không tham số không hiện gì (traces/01-noargs, NOTES.md).
- Dưới Wine phải truyền đường dẫn Windows (`Z:\…`): MFC bỏ ký tự `/` đầu nên `/tmp/x` thành tên tương đối `tmp/x` (NOTES.md, 0x41f127/0x41f288).
- **Mã thoát**: `ExitInstance` 0x41f6f0 trả `[dlg+0x5c]`: 0 khi xong (0x41f979), −2 khi Load/lưu ảnh/ghi .txt lỗi (0x41f966);
  dưới Wine −2 hiện ra là 254 (traces/*/exit.txt). Không có hộp thoại: chuỗi 176/177 ("Cannot convert the file." / "Conversion finished.")
  chỉ được nạp rồi bỏ (timer 0x41ff90).
- **File hỏng**: khi cả hai lần parse đều lỗi, Load đi qua đường giải phóng reader rồi dùng lại (use-after-free, 0x407ed1/0x407ed9 →
  0x407c00 → strcpy 0x429044 đọc reader+0x63d4 đã giải phóng): dưới Wine thấy access violation rồi vẫn thoát 254; trên Windows
  **(đoán)** tiến trình bị giết (WER). Bản Rust: trả −2/254, không ghi file nào.
- Lỗi ghi ảnh hoặc .txt (vd thư mục ra chưa có) cũng cho −2 (driver 0x41f840, 0x41f8c6, 0x41f945). Ảnh `_AVE` đã ghi thì không bị xoá.

## 3. Thiết lập (`ecgviewer.ini`, qua sysdef.dll)

File: thư mục của **EXE** + `\ecgviewer.ini` (`GetModuleFileNameA(NULL)`, sysdef ctor 0x10001010), section `[DMSX]`.
InitInstance đọc các key khởi động **sau** khi parse dòng lệnh và ghi đè cờ (0x41f464…0x41f53b).

| Key | Mặc định khi thiếu | Giá trị → hiệu ứng | Bằng chứng |
|---|---|---|---|
| `IMGSIZE` | "1" (chỉ ký tự đầu − '0') | 0 → 800×600, 3.0 px/mm, font 12×6; 2 → 1280×1024, 4.8 px/mm, font 18×9; khác → 1024×768, 3.8 px/mm, font 14×7 | sysdef getImgSize 0x10003bf0; EXE 0x41f464…0x41f486; Init 0x407940 (0x4079a3…) |
| `IMGTYPE` | "1" | chỉ **ký tự đầu − '0'** (không phải atoi): **0 → JPEG** (.jpg), 2 → PNG (.png), khác (kể cả 1 và giá trị rỗng) → BMP (.bmp); "02" → JPEG | getImgType 0x10003b80; EXE 0x41f4c4…0x41f4e2; đuôi 0x408100 |
| `IMGDENSITY` | "0" | ký tự đầu − '0': 1 → density 2 (bút lưới dày 2; "10" cũng vậy), khác → 1; density 0 không đạt tới được | getImgDensity 0x10003c60; EXE 0x41f4a7…0x41f4ba |
| `LocaleInfo` | — | "japanese" → JP, "english" → EN (không phân biệt hoa thường); thiếu/khác → JP nếu `GetUserDefaultLCID()==0x411`, ngược lại EN | getJapanese 0x10004190, getLocaleInfo 0x10004200; EXE 0x41f490→0x5455e0 |
| `TEXTOUT_IMAGE` (`TEXTOUT_ATTACH` với -T) | **thiếu hoặc rỗng thì ghi "ON" vào INI** (buffer 4 byte: chỉ thấy 3 ký tự đầu, "OFFICE" = OFF) | ≠ "OFF" → ghi .txt | getOutputText 0x100038e0 (ghi 0x10003950); EXE 0x41f4f9/0x41f512 → 0x5456f4 |
| `DEL_IDNAME_IMAGE` (`DEL_IDNAME_ATTACH` với -T) | **thiếu hoặc rỗng thì ghi "ON"** (buffer 4 byte) | "ON" → ẩn giá trị ID/tên (nhãn vẫn vẽ) và bỏ `<ID>_` khỏi tên file; "OFF" → hiện | getOutputFileSecurity 0x10003830; EXE 0x41f528…0x41f53b (cờ = !giá trị) |
| `RhythmLead` | **thiếu hoặc rỗng thì ghi "II"** | I, II, III, aVR, aVL, aVF, V1..V6 (không phân biệt hoa thường) → chuyển đạo của dải nhịp _AVE; không khớp → −1 | getRhythmLead 0x100011c0 (bảng 0x1000d030); EXE 0x40b30f (bảng 0x46f484) |
| `JPEG_QUALITY` | "100" | atoi → chất lượng DIBtoJPG | getJpegQuality 0x10003cd0 |
| `PNG_COMPLEVEL` | "6" | atoi → mức nén DIBtoPNGex | getPngCompLevel 0x10003d00 |
| `GRID_COLOR_JPEG` | 255,64,64 (không ghi) | "%d,%d,%d" màu lưới khi ra JPEG | getGridColorJpeg 0x10003d30; EXE 0x409c6a |
| `GRID_COLOR_EDIT_JPEG` | 0,0,255 | màu lưới JPEG khi record "đã sửa" | getEditGridColorJpeg 0x10003dd0; EXE 0x409c62 |

- Ảnh không phải JPEG: lưới cố định RGB(255,64,64), "đã sửa" RGB(0,0,255) (bút lưới 0x409c10). "Đã sửa" = năm của ngày sửa (tag 7/0xB) ≠ 0 (0x407c00, conv+0x3c).
- Các key EXE không bao giờ đọc (dù INI mẫu có): VERSION, OnLineCSV, ComEnable, ViewMode, GRID_COLOR, Grid, ReportForm(2), BIGGRID(_WIDTH),
  WAVE_PRINT_SENSE, NETWORK_ENABLE_MODE, selForm, latestSearchKind (bảng import, triage.txt).
- `ConvertFileName` (sysdef 0x100042d0) thay `" * . / : ; < > ? \ |` bằng `@` trong tên file (bảng 0x10004308).
- INI mẫu `research/bin/ecgviewer.ini`: IMGTYPE=2 (PNG), IMGSIZE=3 (→ 1024×768), TEXTOUT_IMAGE=ON, DEL_IDNAME_IMAGE=ON, RhythmLead=II, không có LocaleInfo.

## 4. File đầu vào

- Nguồn: máy điện tim gửi qua FTP; tên/đuôi file và số file mỗi lần đo **chưa biết** (chưa có mẫu). Bridge lấy file đầu tiên trong `%TEMP%\ftp`.
- Đọc: reader object 0x100108 byte (ctor 0x4100f0: +0 thư mục char[0x104] luôn rỗng, +0x104 buffer 1 MiB **xoá 0**, +0x100104 số byte đọc);
  Read 0x411400 = `CreateFileA(GENERIC_READ, FILE_SHARE_READ, OPEN_EXISTING)` + `ReadFile` tối đa 0x100000 byte. Mở lỗi vẫn trả "thành công"
  (0x411479→0x4114a9) nên buffer toàn 0 và file bị từ chối ở bước so 12 byte. Số byte thật **không** được parser dùng: mọi giới hạn so với 1 MiB,
  phần sau EOF đọc ra 0. File > 1 MiB bị cắt.
- Chưa có file thật: bố cục dưới đây đọc từ reader và đối chiếu với **bộ ghi** của chính định dạng này còn nằm trong EXE
  (0x410180, header 0x41a540, writer từng block — code của ECG Viewer, không chạy trong EcgPaint).

### 4.1 Bố cục

#### 4.1.1 File, header (block 0) và mục lục (TOC)

| File off | Size | Kiểu | Ý nghĩa | Đọc ở | Lưu vào / dùng ở |
|---|---|---|---|---|---|
| 0x00 | 2 | BE16 | checksum cả file = Σ byte [2, hết) mod 65536 (writer 0x4103d4…0x4103ea) | **không đọc** | — |
| 0x02 | 4 | BE32 | kích thước file − 2 (writer 0x4103c9) | **không đọc** | — |
| 0x03..0x0e | 12 | byte | nếu **bằng** 12 byte ở 0x18..0x23 → từ chối 0xa0005000 (file trắng/toàn 0 rơi vào đây) | 0x410c76 `repe cmpsd` ×3 | — |
| 0x06 | 2 | BE16 | **body block 0** bắt đầu ở đây: chk của block 0 | 0x41a66d | — |
| 0x08 | 4 | BE32 | len block 0 = 0x0a + 6n; (0x06 + len) ≤ 0x100000, không thì 0xa0006001 | 0x41a63a, 0x41a64c | — |
| 0x0c | 2 | BE16 | tag = 0 (writer 0x41a556) | không đọc | — |
| 0x0e | 1 | u8 | phải là 0 hoặc 1, không thì 0xa0006003; writer ghi 0; nghĩa chưa rõ | 0x41a67b | không lưu |
| 0x0f | 1 | u8 | **mã chữ**: 0 = EUC-JP (đổi sang Shift-JIS), 1 = Shift-JIS (chép), 2 → lỗi 0xa0006003, ≥3 = chấp nhận nhưng ký tự 2 byte bị bỏ | 0x41a68f | global 0x52450c (0x40f330) → TEXT 0x40f113 |
| 0x10 | 2 | BE16 | n = số mục TOC (gồm mục 0) | 0x40eea8 | FIND_BLOCK |
| 0x12+6i | 4 | BE32 | offset block i, **tính từ file+6** (= từ body block 0) | 0x40eeeb | body = buf + offset + 6 (0x40eefe) |
| 0x16+6i | 2 | BE16 | tag block i | 0x40eec8 | |

- Mục 0 luôn là {offset 0, tag 0} (body block 0 = file+6). Writer đặt body block thứ nhất ở `file + 6 + (0x0c + 6n)` và các body nối liền
  (0x410248, 0x41038c, 0x41a57f…0x41a5b0). Thứ tự block trong file không quan trọng với reader.
- **FIND_BLOCK** 0x40eea0(buf, tag): duyệt TOC tuần tự, trả `buf + offset + 6` của mục **đầu tiên** có tag khớp, không có → 0.
  Không kiểm tra offset (chỉ block 1 phải < buf+1 MiB, 0x410ca6; block 0: 0x41a64c). Bản Rust nên kiểm tra biên (đọc ngoài file = 0 như bản gốc).

#### 4.1.2 Quy ước chung của mọi block

| Body off | Size | Kiểu | Ý nghĩa | Bằng chứng |
|---|---|---|---|---|
| +0 | 2 | BE16 | chk = Σ byte của `len` byte bắt đầu ở body+2, mod 65536 (so 16 bit) | 0x40ef80; so ở 0x41a66d, 0x41cb97, 0x41973d, 0x4178bd, 0x41aaa0, 0x41a46f, 0x416eb0, 0x4165c7, 0x41d3b3, 0x41bbe4, 0x41842d, 0x4172f1 |
| +2 | 4 | BE32 | len = kích thước block − 2 (đếm từ chính trường len) | writer 0x40ef30 |
| +6 | 2 | BE16 | tag (writer ghi: 1 0x41c969, 2 0x418cd3, 3 0x4176af, 6 0x41a749, 0x103 0x41a321, 0x104 0x416da1); **reader không đọc** | — |

Mã lỗi nội bộ (không đi ra ngoài; mọi mã ≠ 0 dừng parse ở 0x410e88, Load thử kiểu record kế rồi thất bại → exit −2):

| Mã | Khi nào | Bằng chứng |
|---|---|---|
| 0xa0005000 | 12 byte trùng; thiếu block 1 hoặc block 1 ≥ 1 MiB; biến thể lạ; kiểu record ≠ kiểu mong đợi | 0x41116b, 0x410c85, 0x410c9a, 0x410ca8, 0x410d73 |
| 0xa0006001 | thiếu block bắt buộc (0, 2, 3, 6, 8, 0x120) hoặc block 0 vượt 1 MiB; block 0x100 có mà +0x4e sai | 0x41a62a, 0x41a656, 0x41970e, 0x417893, 0x41aa74, 0x41d389, 0x41bf5b |
| 0xa0006002 | checksum sai (mọi block có mặt, kể cả block tuỳ chọn) | các địa chỉ so ở bảng trên |
| 0xa0006003 | header +0x0e ∉ {0,1} hoặc +0x0f = 2 | 0x41a688, 0x41a698 |

#### 4.1.3 Trình tự parse, biến thể và kiểu record

Parse 0x410c50 là phương thức của reader (ecx = reader; tham số record và `lang` = (conv+0x20 ≠ 0) = 1 khi tiếng Anh, 0x407d23…0x407d39):

1. So 12 byte (0x410c76). 2. p1 = FIND_BLOCK(1), phải ≠ 0 và < buf+1 MiB (0x410c8b…0x410ca8). 3. Header block 0 (0x41a610).
4. v = BE16(B1+0x5a) (0x410cc4): 0 → kiểu mong đợi 0; 0xfffe/0xffff → 0xffff; 10 → 10; khác → 0xa0005000 (0x410cd6…0x410d36).
5. Chọn danh sách tag (có block 7 thì dùng 7, không thì 0xB). 6. So `[rec+0]` với kiểu mong đợi (0x410d71). 7. Gọi handler theo danh sách,
dừng ở lỗi đầu tiên; tag 6 bị bỏ khi file có block 0x100 (0x410d86…0x410dc7). 8. Nếu kiểu 0/0xffff: GetAnalText (0x410ebb…0x411126, mục 5.4).

| Biến thể B1+0x5a | Kiểu mong đợi | Record dùng | Danh sách tag (bỏ phần tử 0; bảng) | Ghi chú |
|---|---|---|---|---|
| 0 | 0 | kiểu 1: new 0x655f4, ctor 0x426c00 (`[rec+0]=0`) | 1, 2, 0x103, 6, 0x100, 0x104, 7\|0xB, 0x110, 0x113, 8 (0x46f5f4 / 0x46f60c) | ECG nghỉ "安静時"/"Rest" |
| 0xfffe, 0xffff | 0xffff | kiểu 2: new 0x6561c, ctor 0x426fc0 (`[rec+0]=0xffff`); lần thử kiểu 1 luôn lỗi trước | 1, 2, 0x103, 6, 0x100, **3**, 0x104, 7\|0xB, 0x110, 0x113, 8 (0x46f624 / 0x46f63c) | sau gắng sức "負荷後"/"after EXER" |
| 10 | 10 | không có: lớp kiểu 3 (ctor 0x4273f0) không bao giờ được tạo, setter kiểu 0x426a60 không ai gọi | 1, 2, 0x103, 0x100, 0x120 (0x46f654) | **luôn bị từ chối** (rhythm "リズム") |

Handler (dispatch 0x411188 cho tag 1..0xB, 0x4111cc/0x4111b4 cho tag > 0x100; tag khác bỏ qua):

| Tag | Handler | Thiếu block | Vai trò |
|---|---|---|---|
| 0 | 0x41a610 | 0xa0006001 | header, mã chữ |
| 1 | 0x41cb50 | 0xa0005000 (bước 2) | thiết bị, biến thể |
| 2 | 0x4196e0 | 0xa0006001 | bệnh nhân, ngày giờ ghi |
| 3 | 0x417870 | 0xa0006001 (chỉ kiểu 2) | gắng sức (Master) |
| 6 | 0x41aa20 | 0xa0006001 (chỉ khi không có 0x100) | thiết lập ghi, bộ lọc (dạng gọn) |
| 0x100 | 0x41bba0 | bỏ qua (trả 0) | thiết lập ghi, bộ lọc (dạng đủ) |
| 0x103 | 0x41a420 | bỏ qua | văn bản phụ (chỉ writer dùng) |
| 0x104 | 0x416e50 | bỏ qua (mặc định) | tuỳ chọn F (chỉ vào struct ECAPS) |
| 7 / 0xB | 0x4164f0 / 0x416520 → 0x416550 | bỏ qua | số đo, mã câu kết luận, Minnesota, ngày sửa, chuỗi |
| 0x110 | 0x4183f0 | bỏ qua | số đo theo chuyển đạo |
| 0x113 | 0x4172b0 | bỏ qua | dữ liệu cho GetAnalText |
| 8 | 0x41d370 | 0xa0006001 | **sóng nén** (giải nén ngay trong handler) |
| 0x120 | 0x41e340 | 0xa0006001 | sóng rhythm (chỉ v10 — code chết) |

**Tập block tối thiểu được chấp nhận** (v0): 0, 1, 2, 6 (hoặc 0x100 với +0x4e = 1), 8; mọi block có mặt phải đúng checksum.

#### 4.1.4 Trường văn bản (TEXT 0x40f0a0(dst, dstsz, src, srclen, trimR, skipL))

- memset(dst, 0, dstsz); skipL: bỏ các byte 0x20 đầu. Lặp khi `i < srclen`, `src[i] ≠ 0`, `o < dstsz−1`: byte ≤ 0x80 chép 1 byte;
  byte > 0x80 lấy cặp (src[i], src[i+1]) — **đọc src[i+1] cả khi vượt srclen** — rồi theo mã chữ: 0 → EUC→JIS (−0x8080) → Shift-JIS
  (0x404840 = 0x4048e0(0x404970)); 1 → chép nguyên; khác → không ghi (chuỗi dừng ở đó).
- trimR: từ ô cuối đã dùng lùi về, xoá các byte 0x20 (so 0x3000 ở 0x40f193 không bao giờ đúng ⇒ space toàn góc không bị xoá); gặp ô chưa ghi (0) thì dừng. NUL kết thúc sớm trường.
- Đích là buffer C đã memset 0, mỗi ký tự được strcpy vào ô của nó (0x4046f0 / 0x404670): **chuỗi kết thúc ở byte 0 đầu tiên** — ô bị bỏ qua (mã chữ ≥ 3), byte thứ hai bằng 0 của một cặp, hoặc byte 0 sinh ra khi đổi mã — dù các ký tự sau vẫn được ghi phía sau. Vd mã chữ 3, "AB 漢字 12   " → "AB " (space trước ô trống được giữ). (Kiểm bằng so sánh ngẫu nhiên 2026-11-12.)
- Kết quả trong record **luôn là Shift-JIS (cp932)**. JIS→SJIS: lo' = lo + (hi lẻ ? 0x1f : 0x7d) (+1 nếu ≥ 0x7f); hi' = ((hi−0x21)>>1)+0x81 (+0x40 nếu > 0x9f) với **số học có dấu** (sar: hi < 0x21 cho hi' < 0x81, vd cặp EUC 8E B1 → 'w' AF);
  kana nửa độ rộng EUC (0x8e xx) không được xử lý riêng. Bản Python khớp codec euc_jp→cp932 (scratch/container/selftest.py).
- Writer ECG Viewer luôn ghi mã chữ 0 (0x41a55d) và đệm **space** (0x40f200/0x40f250); trường tên **không trim** ⇒ space cuối được giữ.

#### 4.1.5 Block 1 — thiết bị, biến thể (0x41cb50; size writer 0x64)

| Off | Size | Kiểu | Ý nghĩa | Đọc ở | Lưu vào | Dùng ở |
|---|---|---|---|---|---|---|
| 0x08 | 4 | ASCII | "0100" (writer 0x47b5f4) **(đoán: phiên bản định dạng)** | không | — | — |
| 0x0c | 60 | text, trimR | tên thiết bị **(đoán)** | 0x41cbb8 | B+0x20 char[61] (rec+0x38; 0x42a010) | dòng thiết bị 0x40c9d0 |
| 0x48 | 2 | BE16 BCD | 4 chữ số → (hi, lo), nibble F đọc là 0 **(đoán: phiên bản)** | 0x41cbd7 (0x40eca0) | B+0x08 = hi, B+0x0c = lo (0x429dc0) | 0x40c9d0 `%02d-%02d` |
| 0x4a | 2 | BE16 BCD | 4 chữ số → số → `_itoa` | 0x41cc0c (0x40ede0) | B+0x00 char[8] (0x429d30) | chỉ writer |
| 0x4c | 10 | — | writer ghi 0 | không | — | — |
| 0x56 | 4 | BE32 | chưa rõ | 0x41cc37 | rec+0x2a8 (0x426ba0) | chỉ writer 0x41ca1b |
| 0x5a | 2 | BE16 | **biến thể** (mục 4.1.3) | 0x410cc4 | chọn kiểu record | parse |
| 0x5c | 8 | text | tên loại khám "安静時"/"負荷後"/"リズム" (writer 0x41ca56…0x41ca91) | không | — | — |

#### 4.1.6 Block 2 — bệnh nhân, ngày giờ ghi (0x4196e0; size writer 0x1ae)

Đích C = rec+0x80 (getter 0x426b60), trừ ngày giờ ghi (rec+4). Mọi TEXT ở đây không trim/skip trừ khi ghi khác.

| Off | Size | Kiểu | Ý nghĩa | Đọc ở | Lưu vào (setter) | Dùng ở |
|---|---|---|---|---|---|---|
| 0x08 | 20 | text | **Tên** (không trim) | 0x419765 | C+0x2a char[21] (0x42a7a0; get 0x42a7e0) | nhãn 134 "Name:": .txt 0x40876b, _AVE 0x409fc9, _6CH 0x40bd42 (giá trị chỉ khi cờ ID) |
| 0x1d | 40 | text | chưa rõ (đoán: tên dạng khác) | 0x41978d | C+0x3f char[41] | chỉ writer |
| 0x46 | 20 | text, trimR | **ID bệnh nhân**, buffer 20 ⇒ tối đa **19** ký tự | 0x4197b8 | C+0x00 char[21] (0x42a660; get 0x42a6a0) | tên file `<ID>_` 0x408027; nhãn 133 "ID:" .txt 0x4086ea, _AVE 0x409f35, _6CH 0x40bcf3; GetAnalText +0 (12 ký tự đầu) |
| 0x5a | 1 | ASCII | **Giới tính** '1' → 1 Nam, '2' → 2 Nữ, khác → 0 | 0x4197d1 | C+0x68 u32 (0x42a8c0) | 0x40d5f0: chuỗi 300/301/302 (Male/Female/Unknown) |
| 0x5b | 1 | u8 | ==1 → huyết áp đơn vị kPa | 0x4199f7 | C+0x94 (0x42acf0 tham số 3) | 0x40da40 |
| 0x5c | 2+1+1 | BE16 + u8 + u8 | ngày sinh (năm 0x8000 = không có) | 0x4197f4…0x41983a | C+0x6c Date | chỉ writer |
| 0x60 | 1 | u8 | **đơn vị tuổi**: 0/lạ → năm, 1 tháng, 2 tuần, 3 ngày, 4 giờ; 0x10/0x20/0x30/0x40 → mã 0x10..0x13 hai phần (năm+tháng / tháng+tuần / tuần+ngày / ngày+giờ) | 0x419877 (bảng 0x419e40/0x419e1c) | C+0x78 u32 mã (0x42b940) | 0x40d6d0 (chuỗi 400..404) |
| 0x61 | 3 | text, skipL | tuổi, số 1 | 0x4198f0 | C+0x78+4 char[4] | 0x40d6d0 |
| 0x64 | 2 | text, skipL | tuổi, số 2 (chỉ khi mã ≥ 0x10, không thì "") | 0x41990d | C+0x78+8 char[3] | 0x40d6d0 |
| 0x66 | 1 | u8 | ==1 → đơn vị Anh (inch/pound), khác → cm/kg | 0x41993d | C+0x84 (0x42a940) | 0x40d8c0, 0x40d980 |
| 0x67 / 0x6a | 3 / 1 | text skipL / text | chiều cao phần nguyên / phần lẻ (' ' → "") | 0x419965 / 0x41998c | C+0x88 char[4] / C+0x8c char[2] (0x42aa20) | 0x40d8c0: nguyên ("* " nếu rỗng) [+ "." + lẻ] + "cm"/"inch" |
| 0x6b / 0x6e | 3 / 1 | như trên | cân nặng | 0x4199b5 / 0x4199dc | C+0x8e / C+0x92 (0x42ab80) | 0x40d980: + "kg"/"pound" |
| 0x6f / 0x72 | 3 / 3 | text skipL | huyết áp số 1 / số 2 (đoán: tâm thu/tâm trương) | 0x419a16 / 0x419a2e | C+0x98 / C+0x9c char[4] (0x42acf0) | 0x40da40: `s1/s2 mmHg`; kPa: atoi×0.1 `%3.1f` + " kpa" ("*" nếu rỗng) |
| 0x75 | 1 | u8 | 0..3 → 1..4, khác → 0; chưa rõ | 0x419a4a (bảng 0x419e80) | C+0xa0 | chỉ writer |
| 0x76 / 0x77 | 1 / 22 | u8 / text | **thuốc 1**: mã 1..9 giữ, khác → −1 (0x41a100) / văn bản tự do | 0x419a71 / 0x419a8b | C+0xb8 / C+0xbc (0x42aea0) | 0x40d200: mã → chuỗi 500+mã (Digitalis…Unknown); mã −1 → văn bản; rỗng → 500 "None" |
| 0x8e / 0x8f | 1 / 22 | u8 / text | **thuốc 2** | 0x419aa0 / 0x419ac0 | C+0xd4 / C+0xd8 (0x42af70) | 0x40d200 |
| 0xa6, 0xbd | 22 | text | chưa rõ | 0x419ae7, 0x419b0d | C+0xef, C+0x134 | chỉ writer |
| 0xd4 | 20 | text | **bác sĩ** (医師名/Doctor) | 0x419b33 | C+0x18c (0x42b490; get 0x42b4d0) | dòng "Edit Date…Doctor:" .txt 0x40930e, _AVE 0x40aced |
| 0xe9 | 20 | text | **BS chỉ định** (依頼医師名/Req.Doctor) | 0x419b59 | C+0x162 (0x42b350; get 0x42b390) | .txt 0x408ea6, _AVE 0x40a6e7 (nhãn 159) |
| 0xfe | 20 | text | **kỹ thuật viên** (技師名/Tech.) | 0x419b7f | C+0x177 (0x42b3f0; get 0x42b430) | .txt 0x408e1d, _AVE 0x40a65d (nhãn 160) |
| 0x113 | 10 | text skipL | chưa rõ (đoán: số phòng) | 0x419ba6 | C+0x1a1 | chỉ writer |
| 0x11d, 0x134 | 22 | text | chưa rõ | 0x419bcc, 0x419bf2 | C+0x106, C+0x11d | chỉ writer |
| 0x14b | 40 | text | chưa rõ | 0x419c18 | C+0x1ac | chỉ writer |
| 0x174 | 2 | BE16 | **năm ghi**; 0x8000 = không có → "0000"/"00" | 0x419c2c | rec+0x04 DateTime (0x4292b0) | tên file `%04d%02d%02d%02d%02d%02d` (0x40dd70; năm 0 → chuỗi rỗng); ngày trên trang/.txt 0x40dc00 |
| 0x176..0x17a | 5 | u8 ×5 | tháng, ngày, giờ, phút, giây; giờ > 23 / phút > 59 / giây > 59 → phần giờ **giữ nguyên** (0x4289e6) | 0x419c6c… | rec+4 (+5, +8; Time +0xb) | như trên |
| 0x17c..0x182 | 7 | như trên | ngày giờ thứ hai (chưa rõ) | 0x419d0e… | C+0xa4 DateTime | chỉ writer |
| 0x184 | 40 | text | chưa rõ | 0x419df8 | C+0x1d5 | chỉ writer |

#### 4.1.7 Block 3 — gắng sức (0x417870; bắt buộc với kiểu 2; size writer 0x14)

Đích X = rec+0x654fc (chỉ kiểu 2; getter 0x4273d0; lớp 0x4293b0, 0x28 byte).

| Off | Size | Kiểu | Ý nghĩa | Đọc ở | Lưu vào | Dùng ở |
|---|---|---|---|---|---|---|
| 0x08 | 1 | u8 | ==0xfe → 1 "có thông tin bài test" | 0x4178cb | X+0x00 (0x4295d0) | 0x40d448: chỉ khi 1 mới in "( … [loại])" |
| 0x0a / 0x0b | 1+1 | u8 | phút / giây (chưa rõ) | 0x417b30 | X+0x04 Time | không dùng |
| 0x0c / 0x0d | 1+1 | u8 | **thời gian sau gắng sức** phút / giây | 0x417b30 | X+0x0d Time | 0x40d390: sau chuỗi 175 "after EXER", dạng ` M'SS"` (`" %d'%02d\""`) |
| 0x0e | 2 | BE16 | số b trong "( a/ b[k])"; 0xffff/0x8000 = không có ("*") | 0x41790b | X+0x18 (0x4296d0) | 0x40d4cc |
| 0x10 | 2 | BE16 | số a (đoán: số bước/thời lượng); 0xffff/0x8000 = "*" | 0x417925 | X+0x1c (0x429750) | 0x40d4c1 |
| 0x12 | 1 | u8 | loại bài Master: 1 "D", 2 "T", 0xff "1/2S", 0xfe "1/4S", khác "S" | 0x41793f (bảng 0x41799c/0x417988) | X+0x20 (0x4297d0) | 0x40d451 |
| 0x13 | 1 | u8 | chưa rõ | 0x417978 | X+0x24 | không dùng |

#### 4.1.8 Block 6 — thiết lập ghi dạng gọn (0x41aa20; bắt buộc khi không có block 0x100; size writer 0x46)

| Off | Size | Kiểu | Ý nghĩa | Đọc ở | Lưu vào | Dùng ở |
|---|---|---|---|---|---|---|
| 0x08 | 2 | BE16 | writer ghi 0x0100 (0x41a755) | không | — | — |
| 0x0a | 2 | BE16 BCD | (hi, lo) **(đoán: phiên bản chương trình)** | 0x41aab2 | B+0x18 / B+0x1c (0x429f40) | 0x40c9d0 `" %02d-%02d"` |
| 0x0c | 2 | BE16 | chưa rõ (writer mặc định 80) | 0x41aae7 | B+0x60 (0x42a090) | chỉ writer |
| 0x0e | 1 | bit | bit1 → W+0 bit0, bit0 → W+0 bit1 | 0x41ab08 | W+0 u16 (0x427cc0) | không dùng ở đầu ra |
| 0x0f | 1 | bit7/bit6 | **lọc nguồn**: bit7 → 0 "H50", bit6 → 1 "H60", không → 2 "Hum OFF" | 0x41ab46 | D+0 (0x4299c0) | chỉ nhãn 0x40cfe0 |
| 0x0f | | bit5 | **lọc trôi**: → 1 "d", không → 0 "" | 0x41ab6a | D+4 (0x429a40) | chỉ nhãn |
| 0x0f | | bit4/bit3 | **lọc thông thấp**: bit4 → 0 (25 Hz), bit3 → 1 (35 Hz), không → 5 (OFF) | 0x41ab80 | D+8 (0x429ac0) | wave ctor 0x4057e0 (khi không có -H) + nhãn |
| 0x0f | | bit2 | → 1 | 0x41aba4 | D+0x10 (0x429bc0) | chỉ writer |
| 0x10 | 1 | u8 | 2→0, 4→1, 8→2, 0x20→4, 0x40→5, khác→3 **(đoán: độ nhạy x1/4…x4)** | 0x41abbc | H+0x00 (0x42c090) | không dùng (EcgPaint luôn in "10mm/mV") |
| 0x11..0x15 | | u8/bit/BE16 | thiết lập khác | 0x41ac06…0x41ac5d | H+0x20, +0x34, +0x0c, +0x28, +0x2c | không dùng |
| 0x16+4i | 4 | 2 × BE16 | 12 cặp theo chuyển đạo (thứ tự file I, II, III, aVR, aVL, aVF, V1..V6 → chỉ số lưu trữ qua bảng 0x46f678 − 1) | 0x41ac7f | H lead k (0x42cc90) | không dùng |

D = rec+0x2ac (5 × u32, reset 0 ở 0x429980), H = rec+0x654fc (kiểu 1) / +0x65524 (kiểu 2).

#### 4.1.9 Block 0x100 — thiết lập ghi dạng đủ (0x41bba0; tuỳ chọn; có mặt thì tag 6 bị bỏ)

| Off | Size | Kiểu | Ý nghĩa | Đọc ở | Lưu vào | Dùng ở |
|---|---|---|---|---|---|---|
| 0x0a | 2 | BE16 BCD | (hi, lo) | 0x41bc55 | B+0x18/+0x1c (0x429f40) | 0x40c9d0 |
| 0x10 / 0x12 | 2 / 2 | BE16 | chưa rõ | 0x41bc8a / 0x41bcab | B+0x60 / B+0x64 | chỉ writer |
| 0x14 | 2 | BE16 | **lọc nguồn**: 1 → 0 H50, 2 → 1 H60, khác → 2 Hum OFF | 0x41bccc | D+0 | nhãn |
| 0x16 | 2 | BE16 | **lọc trôi**: 1 → 1 "d", 2 → 2 "D", khác → 0 "" | 0x41bcf5 | D+4 | nhãn |
| 0x18 / 0x1a | 2 / 2 | BE16 | ==1 / ==0xffff | 0x41bd1e / 0x41bd3e | D+0xc / D+0x10 | không |
| 0x20…0x2a, 0x2e, 0x30 | 2 mỗi | BE16 | thiết lập khác (ánh xạ trong notes/waveblocks.md §12) | 0x41bd5e…0x41be4b, 0x41beb0, 0x41bed1 | TL2 (= H) | không |
| 0x2c | 2 | BE16 | **lọc thông thấp**: 1..5 → 0..4 (25/35/75/100/150 Hz), khác → 5 OFF (bảng 0x41bf90) | 0x41be74 | D+8 | wave ctor + nhãn |
| 0x4e | 2 | BE16 | kiểu 0/0xffff: **phải = 1**, không thì 0xa0006001 (kiểu 10: = 3) | 0x41bf46 / 0x41bf11 | — | — |
| 0x50… | | payload | 0x41c0c0(body+0x50): P+0x20/+0x22 → W+0 bit; P+0x24..+0x38 → TL3 (rec+0x655c0); 12 cặp BE16 ở P+0x46+4k → TL2 lead k (bảng 0x46f6b4); **mã trả bị bỏ** | 0x41bf6a | TL2, TL3, W+0 | không dùng ở đầu ra |

#### 4.1.10 Block 0x103 (0x41a420) và 0x104 (0x416e50) — tuỳ chọn, chỉ đi vào struct ECAPS / writer

| Block | Off | Size | Kiểu | Ý nghĩa | Lưu vào | Dùng ở |
|---|---|---|---|---|---|---|
| 0x103 | 0x08 / 0x1c / 0x33 | 20 / 22 / 40 | text | chưa rõ | C+0x15 / C+0x14b / C+0x1fe | chỉ writer |
| 0x104 | 0x08 | 2 | BE16 | phiên bản, phải = 1 (khác → dùng mặc định) (0x416ecb) | — | — |
| 0x104 | 0x0a | 1 | u8 | ==1 → 1 (mặc định 0) | F+0 (rec+0x5e834; 0x427850) | GetAnalText bệnh nhân +0xdd (0x411a5b) |
| 0x104 | 0x0b | 1 | u8 | 1 → 1, 2 → 2, khác → 0 (mặc định 1) | F+4 (0x4278d0) | bệnh nhân +0xde (0x411a6d) |

#### 4.1.11 Block 7 / 0xB — số đo, câu kết luận, chuỗi (0x416550; tuỳ chọn; có block 7 thì dùng 7, không thì 0xB)

M = W+0x5d6c (bảng đo 0x3e0), X7 = W+0x614c (0x18 byte), W = rec+0x5e840. Giá trị BE16 có dấu. Phần đo đọc ở 0x4167f0(body+0x10) (gọi từ 0x4166b4).

| Off | Size | Kiểu | Đơn vị | Ý nghĩa | Đọc ở | Lưu vào | Dùng ở |
|---|---|---|---|---|---|---|---|
| 0x08 | 2 | BE16 | năm | **ngày sửa**; 0x8000 = không có → reset (năm "0000") | 0x4165ff | W+0x63d4 DateTime (0x428500 → 0x4292b0) | cờ "đã sửa" conv+0x3c = năm ≠ 0 (0x407c00): màu lưới; dòng "Edit Date:"; khối nhận xét |
| 0x0a..0x0e | 5 | u8 | | tháng, ngày, giờ, phút, giây | 0x416634…0x416674 | W+0x63d4 | như trên |
| 0x10 | 2 | BE16 s | bpm | **HR** | 0x416822 | M+0x2b8 (0x41682b) | 0x40db80 (sau tag 0x110 được ghi lại nếu có block **7**: 0x416b90) |
| 0x12 / 0x14 / 0x16 / 0x18 | 2 mỗi | BE16 s | ms | **PR / QRS / QT / QTc** | 0x416837…0x416861 | X7+2 / +4 / +6 / +8 | 0x40df70, 0x40dff0, 0x40e070/0x40e100 |
| 0x1a / 0x1c / 0x1e | 2 mỗi | BE16 s | độ | **trục P / QRS / T** | 0x41686f…0x41688b | X7+0xa / +0xc / +0xe | 0x40e150 |
| 0x20 / 0x22 | 2 mỗi | BE16 s | µV | **RV5 / SV1** | 0x416899 / 0x4168aa | M+0xa4 / M+0xcc | 0x40e290, 0x40e3c0 |
| 0x24 / 0x26 / 0x28 / 0x2a | 2 mỗi | BE16 | | chưa rõ | 0x4168bb…0x4168e5 | X7+0x14 / +0x10 / +0x12 / +0x16 | chỉ GetAnalText |
| 0x2c | 0x38 | thô | | chưa rõ (không đổi byte) | 0x4166c7 | W+0x8dc | không dùng trên đường driver |
| 0x64 | 2 | BCD | | (hi, lo) **(đoán: phiên bản chương trình phân tích)** | 0x4169b0 | W+0x914+0 (thô); B+0x10/+0x14 (rec+0x28/+0x2c) | 0x40c9d0 `" %02d-%02d"` thứ ba |
| 0x66+4k | 4 × 12 | u8 + 3 byte | | **mã câu kết luận** k: byte0 cờ, byte1..3 = BE24 các chữ số BCD, F = hết | 0x4169b0 | W+0x914+2+4k (thô) | GetAnalText (mục 5.4); mã in `%-6s` 0x40ceb0 |
| 0x96 | 2 | BCD | | header Minnesota (in `" (%02d-%02d) "`, không xử lý nibble F) | 0x416ac0 | W+0x15e6+0 | .txt 0x409433…0x40946d |
| 0x98+6i | 6 × 8 | 6 byte | | mã Minnesota i: e0 ≠ 0 = có (dừng ở e0 = 0 đầu tiên); e2 = nhóm BCD; e3 = mục (nibble cao) / mục con (nibble thấp, F = không) | 0x416afc | W+0x15e8+6i = {e0,e2,e3,e4,e5,0} | 0x40cb60: `" %d-%d"` / `" %d-%d-%d"` |
| 0xc8 | 2 | BE16 | | **mã khuyến cáo** 0x900..0x919 (DLL tạo câu) | 0x41670b | W+0x1720 | GetAnalText → W+0x1618 → khối nhận xét 0x40de80 |
| 0xca | 1 | u8 | byte | L = độ dài chuỗi ngắn | 0x416c36… | — | con trỏ tiến L+1 |
| 0xcb | ≤255 | text | | chuỗi ngắn: chép **tới NUL đầu tiên hoặc 255 byte** (không theo L) | 0x416c47 | W+0x63e8 char[0x100] | khối nhận xét (khi có ngày sửa) |
| căn chẵn | 2 + n | BE16 s + text | byte | chuỗi dài: n > 2000 → 2000, n < 0 → rỗng; căn theo **địa chỉ tuyệt đối** (= chẵn lẻ của offset trong file) | 0x416c60…0x416cc9 | W+0x64e8 char[0x7d1] | khối nhận xét |

- HR cuối cùng: có block 7 → tag 7 +0x10; chỉ có 0xB và có 0x110 → tag 0x110 (M+0x2b8); chỉ có 0xB → tag 0xB +0x10. W2+8 = (dùng tag 0xB) (0x4165a0).

#### 4.1.12 Block 0x110 — số đo theo chuyển đạo (0x4183f0; tuỳ chọn)

M = đổi BE→LE thẳng body+8..+0x3e5 vào W+0x5d6c (cùng offset tương đối), trừ một BE32.

| Off | Size | Kiểu | Ý nghĩa | Đọc ở | Lưu vào | Dùng ở |
|---|---|---|---|---|---|---|
| 0x008 | 0x2a0 | 28 mảng × 12 BE16 | số đo theo chuyển đạo, **thứ tự hiển thị** I..aVF, V1..V6 (suy từ RV5 = mảng 6 phần tử 10, SV1 = mảng 8 phần tử 6) **(đoán về tên mảng)** | 0x418480 | M+0x000 | EcgPaint chỉ đọc M+0xa4 (RV5) và M+0xcc (SV1) |
| 0x2a8 | 4 | BE32 | chưa rõ | 0x418480… | M+0x2a0 | GetAnalText |
| 0x2ac | 0x1a | 13 BE16 | phần tử 10 (M+0x2b8) = HR | 0x418480… | M+0x2a4 | HR; nếu có block 7 thì HR bị ghi lại từ tag 7 (0x418847 → 0x416b90) |
| 0x2c6 | 0x120 | 4 × 3 × 12 BE16 | chưa rõ | 0x4187ea…0x418839 | M+0x2be | GetAnalText |
| 0x3e6 | 0x270 | 26 × 12 BE16 | chưa rõ | 0x4189a0 | W+0x6164 | không dùng trên đường driver |

#### 4.1.13 Block 0x113 — dữ liệu cho GetAnalText (0x4172b0; tuỳ chọn)

| Off | Size | Ý nghĩa | Đọc ở | Lưu vào |
|---|---|---|---|---|
| 0x08 | 2 + 12 × 12 | 2 byte thô + 12 mục (4 byte thô + 4 BE16) | 0x417410 | W+0x1554 (0x92 byte) |
| 0x9a | 2n+2 | danh sách A: BE16 tới 0xffff, ≤ 0xa8 phần tử | 0x4174f0 | W+0xc |
| tiếp | 16n | danh sách B: mục 8 × BE16, hết khi word đầu = 0xffff, ≤ 0x78 mục | 0x417580 | W+0x15c |

Chỉ DLL đọc (qua anal+0x90c/0x910/0x97a), và chỉ cho phần chi tiết `::[…]` mà EcgPaint tắt ⇒ **không ảnh hưởng đầu ra**.

#### 4.1.14 Block 8 — sóng nén (0x41d370; bắt buộc)

| Off | Size | Kiểu | Ý nghĩa | Đọc ở | Đi đâu |
|---|---|---|---|---|---|
| 0x06 | 8 | — | không đọc (tag + 6 byte) | — | — |
| 0x0e | 2 | BE16 | **loại nén** t: 1 → method 1, 2 → 4, 3 → 5, 4 → 16, khác (vd 0) → **method 0** | 0x41d84f (bảng 0x41d9f0) | CmpData+0x31cf0 (u16); R+0 = {1:1, 2:2, 3:4, 4:3, khác 0} (0x41d9a1, chỉ code ngoài luồng đọc) |
| 0x10 | 2 | BE16 | **x0** = mẫu đầu của cửa sổ nhịp TB (0..599) | 0x41d426 | MH+0 → W+0x1724 |
| 0x12 | 2 | BE16 | **x1** = mẫu cuối (vẽ x1 − x0 mẫu) | 0x41d43a | MH+2 → W+0x1726 |
| 0x14 | 1 | u8 | cờ phần A: bit0 A1, bit1..4 A2[0..3] (dừng ở bit thiếu đầu tiên), bit5 A3 | 0x41d558 | |
| 0x15 | 1 | — | bỏ qua | — | |
| 0x16… | 16 / 0x58 / 16 | | A1 (mốc nhịp TB Fm, nếu bit0: {src[1], src[2], 6 × BE16 từ src+4} — +8 QRS onset, +0xa QRS offset theo DLL), A2[i] (struct đo: 16 byte như A1 + 3 mảng 12 × BE16), A3 (Fm thứ hai); thiếu → 2 byte đầu 0xff 0xff | 0x41d550 (0x41d660, 0x41d700) | MH+4/+8/+0xc → W+0x173c/+0x1758/+0x1906 |
| tiếp | 2 + 4n | BE16 n + n × {BE16 pos, u8 b2, u8} | **danh sách QRS**: chép ≤ 0x100 mục, con trỏ vẫn tiến 4n+2 | 0x41d780 | MH+0x10 → W+0x1922 |
| tiếp | 2 + 4n | như trên | **danh sách 2** (≤ 0x200 mục) | 0x41d780 | MH+0x14 → W+0x1d26 |
| D+0 | 2 | — | không đọc | — | |
| D+2 | 2 | BE16 | **count** = số mẫu nhịp mỗi chuyển đạo trong luồng (method 5: ở nửa tần số) | 0x41d88f | hdr+2 (CmpData+0x31cf6) |
| D+4… | 9 khối | | khối i = BE16 len (**byte, gồm 2 byte len**) + dữ liệu + 1 byte đệm nếu len lẻ; i = 0: khối chính (nhịp TB…), i = 1..8: I, II, V1..V6 | 0x41d8b6…0x41d914 | hdr+0x10+4i = con trỏ tới **trường len** của khối i |

- Với cờ = 0 và hai danh sách rỗng: D = body+0x1a, count ở +0x1c, khối 0 ở +0x1e.
- Struct tạm S (new 0x465c, 0x41d3c1) giữ MH (S+0x3854) và các vùng A/B/C mà DLL đọc qua con trỏ; sau giải nén chép sang W+0x1724… (0x41d4d7, 0x4281a0).
- Sau khi gọi DLL, EcgPaint đảo byte tại chỗ trường len của 9 khối trong buffer (0x41d92a…0x41d961) — vô hại. **Mã trả của Decomp12lead bị bỏ**
  (0x41d92a `xor esi,esi`; handler luôn trả 0, 0x41d4e5).

#### 4.1.15 Block 0x120 — sóng rhythm (0x41e340; chỉ biến thể 10 ⇒ code chết)

Mô tả ngắn để nhận ra nếu gặp: +0xe BE16 thời lượng (giây); +0x10 BE32 ≠ 0; +0x14/+0x18/+0x1c/+0x20/+0x24 BE32 offset (từ body) tới 2 bảng,
2 sóng ≤ 90000 mẫu và khối kênh {BE16 mã kênh, BE16 cờ nén, BE32 len, dữ liệu, đệm lẻ}: cờ 1 → DecompressWave (Huffman bảng mặc định,
wrapper 0x404490), khác → BE16 thô (0x41eac0…0x41eba3). Handler ghi vào offset của record kiểu 3 trên record kiểu 1/2 (tràn heap). Bản Rust:
từ chối biến thể 10 như bản gốc.

#### 4.1.16 Record trong bộ nhớ (mô hình cho bản Rust)

| Record off | Size | Lớp (ctor) | Getter | Nội dung / nguồn |
|---|---|---|---|---|
| +0x000 | 4 | u32 | 0x426aa0 | kiểu: 0 (nghỉ) / 0xffff (sau gắng sức) |
| +0x004 | 0x14 | DateTime 0x429120 | 0x426ae0 → get 6 chuỗi 0x429360 | ngày giờ ghi (B2+0x174); Date {+0 "%04d", +5 "%02d", +8 "%02d"}, Time ở +0xb {"%02d" ×3} |
| +0x018 | 0x68 | B "phiên bản" 0x429c20 | 0x426b20 | +0 char[8] (B1+0x4a), +8/+0xc (B1+0x48), +0x10/+0x14 (tag 7 +0x64), +0x18/+0x1c (B6/B100 +0xa), +0x20 char[61] tên thiết bị, +0x60/+0x64 |
| +0x080 | 0x228 | C bệnh nhân 0x42a170 | 0x426b60 | block 2 (mục 4.1.6); +0x15/+0x14b/+0x1fe từ block 0x103 |
| +0x2a8 | 4 | u32 | 0x426be0 | B1+0x56 |
| +0x2ac | 0x14 | D bộ lọc 0x4298b0 | 0x426e60 / 0x427250 | +0 nguồn (get 0x429a00), +4 trôi (0x429a80), +8 thông thấp (**0x429b00**), +0xc, +0x10 |
| +0x2c0 | 0x5e574 | R sóng 0x42d9b0 | 0x426ea0 / 0x427290 | +0x10 u16 N; +0x14 N × 32 byte (≤ 12000) — mục 4.2 |
| +0x5e834 | 0xc | F tuỳ chọn 0x427740 | 0x426ee0 / 0x4272d0 | +0 / +4 (block 0x104, mặc định 0 / 1), +8 = dùng tag 0xB |
| +0x5e840 | 0x6cbc | W đo đạc 0x4279b0 / 0x428750 | 0x426f20 / 0x427310 | +0x914 mã câu, +0x948 12 dòng câu (0xc0c), +0x15e6 Minnesota, +0x1618 câu khuyến cáo (0x108), +0x1720 mã khuyến cáo, +0x1724 MH, +0x252a nhịp TB, +0x5d6c M, +0x614c X7, +0x63d4 ngày sửa, +0x63e8/+0x64e8 chuỗi |
| +0x654fc | 0xc4 / 0x28 | H thiết lập (kiểu 1) / X gắng sức (kiểu 2) | 0x426f60 / 0x4273d0 | kiểu 2: H ở +0x65524, I ở +0x655e8 |
| +0x655c0 | 0x34 | I (TL3) | 0x426fa0 | payload block 0x100 |

Cỡ: kiểu 1 = 0x655f4, kiểu 2 = 0x6561c. Hai kiểu dùng chung offset cho D, R, F, W (getter 0x426e40=0x427230, 0x426e80=0x427270, 0x426ec0=0x4272b0,
0x426f00=0x4272f0; Load 0x407daa…0x407e37).

### 4.2 Sóng (payload của block 8)

#### 4.2.1 Hợp đồng gọi giải nén

- Wrapper ECAPS tĩnh 0x47f5b8 (ctor 0x403bc0 lúc khởi động: `LoadLibrary("t_ecaps12c.dll")` + 18 `GetProcAddress` + Init(0); thiếu DLL thì vẫn
  gọi Init qua con trỏ NULL **(đoán: crash khi khởi động)**). Handler tag 8 gọi Init(0) (0x41d81e), dựng arg `{MH*, hdr*, u16 method}` trên stack
  rồi gọi wrapper 0x404380 (0x41d925): chép MH 0x18 byte → meas+0xee8, hdr 0x34 byte → cmp+0x31cf4, method → cmp+0x31cf0, gọi
  DLL `Decomp12lead(wave, meas, cmp)` (DLL 0x10003ffe, stdcall 3; EXE 0x4043c9), ghi 0x3842 vào header nhịp TB, trả {wave, meas+0xf00}.
- **hdr** (0x34 byte, dựng ở 0x41d827…0x41d914): +0 u16 0; +2 u16 **count** (D+2); +4/+6 u16 x0/x1 (EXE chép từ MH ở 0x41d8a5…0x41d8af; method 5 ghi đè 125 − x0 / x1 + 125); +8..+0xf 0;
  +0x10 con trỏ khối chính; +0x14+4L con trỏ khối lead L (L = 0..7 = I, II, V1..V6). Mọi khối bắt đầu bằng BE16 độ dài **byte gồm chính nó**.
- **MH** (0x18 byte): +0 u16 x0, +2 u16 x1, +4 ptr Fm (A1), +8 ptr struct đo (A2), +0xc ptr Fm2 (A3), +0x10 ptr danh sách QRS, +0x14 ptr danh sách 2.
- Đầu ra: **wave** {+0 ptr, +4 u16 dung lượng (12000), +8 ptr buffer, +0xc u16 số record N} → `R.0x42dba0` chép N = min(N, 12000) record
  vào rec+0x2c0+0x14 (setter 0x42dae0); **AVG** (meas+0xf00, 0x3842 byte) → `W.0x4282d0` chép vào W+0x252a (0x41d992).
- **Mã trả bị bỏ** (mục 4.1.14). Khi method lỗi, DLL không tính chuyển đạo dẫn xuất (DLL 0x100040d8) và phần chưa giải giữ giá trị cũ
  (buffer wave xoá 0 ở Init 0x403f25; meas #3 không được Init — object tĩnh nên là 0 lúc chạy đầu). Bản Rust nên giữ đúng hành vi "không báo lỗi".

#### 4.2.2 Các method (chi tiết bit: notes/decomp.md §4–§8, §12; bộ giải tham chiếu `research/py/decomp_decode.py`, bộ nén `decomp_encode.py`)

| t (B8+0xe) | Method | Khối chính (nhịp TB 8 lead × 600, lead-major) | Khối lead L (count mẫu) | Điều kiện / lỗi | Bằng chứng |
|---|---|---|---|---|---|
| khác 1..4 | 0 | 4800 × BE16 thô | count × BE16 thô | len−2 phải **đúng** 9600 / 2·count, sai → cột giữ nguyên + cờ lỗi; khối rỗng (len ≤ 2) → lead = 0; count > 12000 → −32 | DLL 0x10005c6c, 0x10005d76, 0x10005de2 |
| 1 | 1 | Huffman bảng mặc định (0x10019e88) của 4800 hiệu (tổng dồn chạy liên tục qua ranh giới lead) | Huffman của count hiệu | hết byte → phần còn lại 0, lỗi | DLL 0x10005a60, 0x10005b75, 0x100058e8 |
| 2 | 4 | 4800 mã unary/ESC, lưu 4d, tích phân bậc 2 mỗi lead | count/2 mã ở nửa tần số (chuỗi 0, ±1..3, ESC), lưu 4d, Σ, lên mẫu ×2 tuyến tính, + template quanh QRS, ×2 (trừ hàng 0) | cần danh sách QRS + F cho template | DLL 0x10001a3c, 0x10001c9e, 0x10001d82, 0x10001ef7 |
| 3 | 5 | như 4 nhưng 8 × 300, rồi Catmull-Rom 300 → 600 (÷128) | như 4 (0 = một số 0), Catmull-Rom ×2 (÷64) ⇒ **N = 2·count**; mọi mốc thời gian nhân đôi | cap ≥ 2·count | DLL 0x10001ab0, 0x10006518, 0x100013cc |
| 4 | 16 | bảng Huffman trong luồng + 8 đoạn trên cửa sổ [x0, x1] (x0 < x1 < 600), TB = 2·ΣΣd, ngoài cửa sổ 0 | đoạn type 0..3 (nguyên/nửa tốc độ/+template), nhịp = 8·Σd (+2·AVG); 3 cột cờ (word 1..3) | khối chính ≤ 6000 word; khối lead ≤ count word; −64 nếu count > cap | DLL 0x10003df7, 0x100026b2, 0x100027ac, 0x100037cb, 0x10002d63 |

- Huffman bảng mặc định (method 1): bit MSB trước; mã → (base, nbits); nbits bit thêm là số bù 2 cộng vào base; mẫu = tổng dồn int16 từ 0.
  Ví dụ: giá trị 0, 1, −1, 2 → hiệu 0, 1, −2, 3 → bit `01 111 1011 0001` → byte `7d 88` (decomp_test.py). Bảng đủ: notes/decomp.md §5.1, `scratch/decomp/huff_default.tsv`.
- Method 4/5/16 cộng template nhịp TB vào nhịp rồi nhân 2 ⇒ **1 LSB nhịp TB = 2 LSB nhịp** (DLL 0x10001ed5, 0x1000381c, 0x10003acd).
- Không biết máy thật dùng method nào (câu hỏi mở). Code Python đã kiểm 27/27 trên tín hiệu tổng hợp (bộ nén và bộ giải cùng viết từ một mô tả;
  chưa đối chiếu với DLL thật — synthetic-plan.md).

#### 4.2.3 Mẫu nhịp (R = rec+0x2c0) và nhịp trung bình (W+0x252a)

| Thành phần | Bố cục | Bằng chứng |
|---|---|---|
| R | +0 u32 mã; +4 ptr → +0x14; +8 u16 12000; +0xc ptr; **+0x10 u16 N**; +0x14 buffer N × 32 byte | reset 0x42d9f0; setter 0x42dae0; getter N 0x42dbd0 |
| record 32 byte = 16 × int16 LE | byte 0: cờ QRS = (b2&3)+1 tại vị trí QRS; byte 1: 0x80 tại mục danh sách 2; word 1..3: cờ method 16; **word 4..11 = I, II, V1, V2, V3, V4, V5, V6**; **word 12..15 = III, aVR, aVL, aVF** | DLL 0x1000645d, 0x1000322a, 0x10003c2a, 0x100042c6; EXE 0x405c47 |
| N | = min(count, 12000) (method 5: 2·count) | 0x42daf6…0x42db24 |
| nhịp TB (0x3842 byte) | u16 0x3842, rồi int16[12][600] **lead-major theo chỉ số lưu trữ** (lead s mẫu k ở +2+2·(600s+k)) | 0x4282a0; DLL 0x10004266 |
| cửa sổ nhịp TB | MH x0, x1 (W+0x1724/+0x1726): _AVE vẽ x1 − x0 mẫu từ mẫu x0 | 0x4063db…0x4063ee, 0x4067a8…0x4067c7 |

- **Chỉ số lưu trữ**: 0 I, 1 II, 2..7 V1..V6, 8 III, 9 aVR, 10 aVL, 11 aVF. **Thứ tự hiển thị**: I, II, III, aVR, aVL, aVF, V1..V6.
  Bảng hiển thị→lưu trữ 0x46f270 = 0x46f484 = {0,1,8,9,10,11,2,3,4,5,6,7}; lưu trữ→hiển thị 0x46f2a0 = {0,1,6,7,8,9,10,11,2,3,4,5};
  tên chuyển đạo ở wave+0x208 (JP: I/II/III là ký tự Shift-JIS 0x8754/0x8755/0x8756, 0x405a49). Khối nén i (1..8) → word 4+(i−1).
  Với mục đích tái tạo EcgPaint, ánh xạ này chắc chắn; máy có ghi V1 vào khối 3 hay không thì chỉ mẫu thật trả lời (I/II chắc nhờ công thức dẫn xuất).
- Giá trị 0x8000 = mẫu không hợp lệ: hàm dịch đường đáy bỏ qua lead có min/max = 0x8000 (0x405eb4, 0x405ec3).
- Cờ byte 0..7 **không được hàm vẽ nào đọc** (vẽ chỉ đọc word 4+s: 0x405e0f, 0x4060b8, 0x406cd6, 0x4071ed).

#### 4.2.4 Đơn vị

| Đại lượng | Giá trị | Mức tin cậy | Bằng chứng |
|---|---|---|---|
| Tần số lấy mẫu | **500 Hz** | (đoán mạnh) | bước vẽ 20.0/px_per_mm mẫu mỗi pixel (hằng 0x46f3f8 = 20.0; 0x4066f2, 0x406cea, 0x40720f) với nhãn cố định "25mm/s"; dải nhịp 5000 mẫu (0x406ab7) = 10 s = 250 mm; cột _6CH 2500 mẫu (0x406e63) = 5 s |
| Biên độ nhịp | **1.25 µV/LSB** (80 LSB/mm ở 10 mm/mV) | (đoán) | LSB/pixel = 20.0/px_per_mm × 4.0 (hằng 0x46f458; 0x406b7e…0x406b9b, 0x406f38…0x406f55); đường đáy 0x405eca dùng 80.0/px |
| Biên độ nhịp TB | **2.5 µV/LSB** (40 LSB/mm) | (đoán) | 20.0/px_per_mm × 2 (`fadd st0,st0` 0x4064bc); tỉ lệ 2:1 khớp độc lập với DLL (mục 4.2.2) |
| Độ dài | dải nhịp vẽ min(N, 5000) mẫu đầu; _6CH vẽ lead chi ở mẫu 0..2499, lead ngực ở 2500..4999 (đọc quá buffer nếu N < 5000); nhịp TB 600 mẫu (1.2 s, mốc 250 = 0.5 s) | chắc (code) | 0x406ab7…0x406ac9, 0x406e63…0x406e77, 0x4070ee…0x4071ed |

Giá trị tuyệt đối (µV/LSB, Hz) phải được xác nhận bằng golden (synthetic-plan.md: xung chuẩn 1 mV × 200 ms).

#### 4.2.5 Chuyển đạo dẫn xuất (số nguyên 16 bit, `>>` = dịch số học)

| Nơi tính | III | aVR | aVL | aVF | Bằng chứng |
|---|---|---|---|---|---|
| DLL (nhịp và nhịp TB, chỉ khi method trả 0) | II − I | (−(I + II)) >> 1 (cộng 32 bit) | I − (II >> 1) | II − (I >> 1) | DLL 0x100042e7…0x10004310, 0x1000427f…0x100042b3 |
| EXE (nhịp, sau lọc, mọi mẫu) | II − I | −((I + II) >> 1) | I − (II >> 1) | II − (I >> 1) | EXE 0x405c47…0x405c82, 0x405ce3…0x405d25 |

Hai công thức aVR lệch 1 LSB khi I + II lẻ. Nhịp hiển thị dùng công thức EXE (tính lại sau lọc); nhịp TB hiển thị dùng công thức DLL (không lọc).

## 5. Xử lý

### 5.1 Trình tự sau parse

1. Load 0x407c70: parse thành công → wave object `new 0x248`, ctor 0x4057e0(R, F = D, avg = W+0x252a, hdr = W+0x1724) (0x407e6e…0x407e78).
2. Ctor: chọn bộ lọc (5.2), `0x405bf0(N = R+0x10, src = R+0x14, idx)`: lọc 8 kênh vào buffer mới [wave+0x19c] (N × 32 byte) và tính lại dẫn xuất.
3. Dịch đường đáy: 0x405db0(trunc(40·px)…, px) → wave+0x1a0 (dải nhịp _AVE) và 0x406010(170·px…) → wave+0x1d0 (_6CH) (Load 0x407e83):
   với lead hiển thị i, min/max lấy trên mẫu 1 ∪ mẫu [(N/2)·(i/6)+1, 2·(N/2)·(i/6)−1] — **lead chi (i < 6) chỉ dùng mẫu 1**, lead ngực dùng nửa sau
   (0x405e25…0x405e9a); offset = −((trunc(min/(80/px)) + trunc(max/(80/px))) >> 1), kẹp theo kích thước khung (0x405f11…0x405f3f).
   Cách dùng offset trong trang: chủ đề render (đo bằng trace GDI, mục 6.6).
4. Cờ "đã sửa" conv+0x3c = năm ngày sửa ≠ 0 (0x407c00) → màu lưới.

### 5.2 Chọn bộ lọc thông thấp

| Ưu tiên | Nguồn | Ánh xạ → idx | Bằng chứng |
|---|---|---|---|
| 1 | `-H<n>` ([0x5456fc], mặc định −1) | 0 → 5 (OFF), 1 → 0 (25 Hz), 2 → 1 (35), 3 → 2 (75), 4 → 3 (100), 5 → 4 (150), khác → −1 | 0x405770, bảng 0x4057a8 |
| 2 | D+8 từ block 0x100 +0x2c | 1..5 → 0..4, khác → 5 | 0x41be74, bảng 0x41bf90 |
| 2' | D+8 từ block 6 +0x0f (khi không có 0x100) | bit4 → 0, bit3 → 1, không → 5 | 0x41ab80…0x41ab9f |

Bridge không truyền `-H` ⇒ thực tế bộ lọc do file quyết định. **Lọc nguồn (H50/H60) và lọc trôi chỉ dùng cho nhãn**, không áp lên sóng
(refs 0x429a00/0x429a80: chỉ 0x40cfe0 và hàm ghi ngoài luồng).

### 5.3 Bộ lọc (0x405bf0; mỗi mẫu 8 kênh word 4..11; thang ×32 trong int32)

- Mẫu 0: bảng 0x46f2d0[idx] = {0x426270, 0x426470, 0x426670, 0x425f00, 0x426050}: ra = vào, prev = 32x, trạng thái như tín hiệu hằng x
  (acc8 = 8x, ring8 = x; ring4 = x, acc4 = 4x, s35 = 16x; h75 = (x, x); s100 = 16x).
- Mẫu 1..N−1: bảng 0x46f2e4[idx] = {0x426380, 0x426560, 0x426710, 0x425f90, 0x4260d0}, gọi với (src+32i+8, dst+32i+8, wave+0x18, 8).
- idx 5 (OFF): chép nguyên 32 byte (`rep movsd` 8). Mọi idx: sau mỗi mẫu tính lại word 12..15 bằng công thức EXE (4.2.5).
  Khi có lọc, word 0..3 của buffer đích **không được ghi** (rác heap, không ai đọc).

| idx | Nhãn | t (giá trị ×32 trước làm mượt), x = mẫu vào int16 | Hàm truyền gần đúng |
|---|---|---|---|
| 0 | " 25Hz" | acc8 += x − ring8[r]; ring8[r] = x; t = 4·acc8; r = (r+1)&7 | trung bình trượt 8 mẫu |
| 1 | " 35Hz" | acc4 += x − ring4[r]; ring4[r] = x; t = s35 + 4·acc4; s35 = t >> 1; r = (r+1)&3 | (1/8)(1+z⁻¹+z⁻²+z⁻³)/(1−½z⁻¹) |
| 2 | " 75Hz" | t = 8·(x + 2·x₁ + x₂); (x₁, x₂) ← (x, x₁) | ¼(1+2z⁻¹+z⁻²) |
| 3 | "100Hz" | e = 18x − (s100 >> 3); t = s100 + e; s100 = e | ≈0.5625(1+z⁻¹)/(1+0.125z⁻¹) |
| 4 | "150Hz" | t = 32x | chỉ làm mượt |

- **Làm mượt chung 0x426820** (k = 128, hằng 0x46f9dc…0x46f9ec): d = t − prev; d ≥ 512 → y = t − 384; d ≤ −512 → y = t + 384;
  còn lại y = (t + 3·prev) >> 2; prev = y; ra = y >> 5 (cắt int16). Bước nhỏ bị làm mượt, sườn lớn đi qua với trễ cố định ⇒ cả "150Hz" cũng đổi tín hiệu nhỏ.
- Mô hình đã đối chiếu từng lệnh: `research/py/waveblocks_filters.py` (selftest).
- Nhịp TB **không** lọc.

### 5.4 Câu kết luận: GetAnalText (DLL), số đo: chép từ file

- Sau vòng tag (kiểu 0/0xffff), parse tạo wrapper ECAPS thứ hai (`new 0xa4f54`, LoadLibrary lần 2), `Init(sel)` với **sel = 0 khi JP, 1 khi EN**
  (0x410f11…0x410f2a; sel = (conv+0x20 ≠ 0); Init 0x407940 đặt conv+0x20 = 10000 khi đối số `jp` = 0 (0x407963); driver truyền `jp` = [0x5455e0]
  (0x41f76f) = (LocaleInfo là tiếng Nhật) (0x41f4a1)), dựng struct bệnh nhân 0x100 byte (0x411990), rồi wrapper 0x404170 → DLL `GetAnalText`
  (DLL 0x10003f36). Kết quả: 12 dòng → W+0x948 (0xc0c), câu khuyến cáo → W+0x1618 (0x108) (0x41104a…0x411126). Wrapper bị huỷ (0x411154).
- Đầu vào thực sự dùng (cấu hình EcgPaint: chi tiết `::[…]` tắt, số mã tắt — EXE 0x403e48…0x403ecf; notes/decomp.md §11):
  12 mã {u8 cờ, BE24 mã} từ tag 7 +0x66 và mã khuyến cáo từ tag 7 +0xc8. Số đo (M, X7), bệnh nhân, block 0x113 **không** ảnh hưởng văn bản.
- Thuật toán (DLL 0x1000442f, 0x100044b3, 0x10005525):
  - cờ hợp lệ khi b ≠ 0, bit7 = 0, (b&0x40 ⇒ b&0x08), (b&0x20 ⇒ b&0x0c), (b&0x10 ⇒ b&0x0e); mã có cờ không hợp lệ bị bỏ;
  - mã 9xxxxx (câu phân loại) đầu tiên được dời xuống cuối; các dòng còn lại giữ thứ tự;
  - dòng = ('#' nếu cờ&8, '+' nếu cờ&4, ' ' nếu khác) + văn bản của mục **cuối cùng** trong bảng mã 0x10007004 có mã đó, lấy từ cột 6 tới trước `::`
    hoặc tới ký tự khác space cuối; mã không có trong bảng → chỉ còn ký tự đầu;
  - bảng câu: JP 0x10008648 / EN 0x10008024 (393 con trỏ: 322 câu + 71 cụm từ); khuyến cáo: JP 0x10017c58 / EN 0x10017bec, mục = mã − 0x900 (< 26),
    bỏ 6 ký tự đầu "NNNN  ", cắt space cuối.
- Dump bảng và mô phỏng: `research/py/decomp_analtext.py`, `scratch/decomp/analtext*.tsv`. Ví dụ: {0x08, 0x0102ff} → "#ARTIFACT PRESENT" (EN),
  {0x02, 0x9110ff} → " **  normal ECG  **".
- Số đo (HR, PR, QRS, QT/QTc, trục, RV5/SV1) **chỉ được chép** từ tag 7/0x110 và định dạng (mục 6.4). EcgPaint không đo gì.
- Trong DLL, Analysis/ExAnalysis/Measure/… là stub (0x10003f21, 0x10003f28, 0x10003ff0). **Bản Rust không cần DLL**: cần bộ giải nén (4.2.2)
  và bảng câu (đọc từ DLL lúc chạy, hoặc nhúng — xem câu hỏi mở về bản quyền dữ liệu).

## 6. Đầu ra

### 6.1 Thư mục, tên file, định dạng

| Mục | Hành vi | Bằng chứng |
|---|---|---|
| Thư mục | `GetTempPathA()` + `EcgViewer\fileout` + `\`; **không tạo thư mục** (CreateDirectoryA chỉ có ở 0x414ea0, không thuộc tập hàm driver gọi tới) ⇒ thiếu thư mục thì lưu lỗi → exit −2 | 0x41fa80, chuỗi 0x47bb04; `pe_code reach 0x41f710` (scratch/spec/reach_driver.txt) |
| Dưới Wine | `%TEMP%` = `$WINEPREFIX/drive_c/users/<user>/Temp` (không phải `/tmp`) — Bridge Linux đọc `/tmp/EcgViewer/fileout` **(chưa kiểm chứng bằng file thật)** | NOTES.md "Môi trường Wine" |
| Tên gốc (base) | (cờ ID = 1 **và** ID khác rỗng ? `<ID>` + "_" : "") + ngày giờ ghi `%04d%02d%02d%02d%02d%02d` (năm = 0 → chuỗi rỗng), rồi `ConvertFileName` (thay `" * . / : ; < > ? \ \|` bằng `@`) | 0x407ff0 (0x408014…0x408046), 0x40dd70 (chuỗi 0x47b4b8), sysdef 0x100042d0 |
| File | `<base>_AVE<ext>` (0x408180), `<base>_6CH<ext>` (0x408290), `<base>.txt` (0x4083a0) | driver 0x41f7d0, 0x41f856, 0x41f8e5 |
| Đuôi / bộ mã hoá | fmt 1 → `.jpg`, `DIBtoJPG(path, dib, JPEG_QUALITY, 1)`; fmt 2 → `.png`, `DIBtoPNGex(path, dib, {0x81, PNG_COMPLEVEL})`; khác → `.bmp`, `DIBtoBMP(path, dib)`; ảnh lấy bằng `DCtoDIB(memdc, 0, 0, w, h)`; trả ≠ 0 là thành công | 0x408100, 0x4084d0 (0x408539…0x408598) |
| Kích thước ảnh | IMGSIZE → 800×600 / 1024×768 / 1280×1024; px/mm 3.0 / 3.8 / 4.8; bitmap `CreateCompatibleBitmap(CreateDCA("DISPLAY"), w, h)` (độ sâu màu theo màn hình) | Init 0x407940 (0x4079a3…), 0x409b4f |
| Font | EN: Arial, ANSI_CHARSET; JP: "MSゴシック", SHIFTJIS_CHARSET (0x80); weight 500 (MSゴシック ở IMGSIZE 2: 600), OUT_TT_PRECIS (4), pitch 0x21; cao×rộng 12×6 / 14×7 / 18×9 theo IMGSIZE | Init 0x407940 (0x4079c3…0x407ac0; chuỗi 0x47b244, 0x47b234) |
| Bút lưới | JPEG: `GRID_COLOR_JPEG` / `GRID_COLOR_EDIT_JPEG` khi "đã sửa"; định dạng khác: RGB(255,64,64) / RGB(0,0,255); độ dày 2 nếu density 2, không thì 1 (bút dày 2 của GDI, đo trên Windows 11 ở cả ba cỡ ảnh: đoạn ngang = hàng y từ x_min và hàng y−1 từ x_min+1; đoạn dọc = hai cột x−1 và x; Wine vẽ khác) | 0x409c10 (0x409c62, 0x409c6a) |
| Bút sóng | `0x405350(0x402000, 1)` → mức xám = 0x402000 & 0xff = 0 ⇒ đen, 1 px ở mọi chế độ **(đoán: chủ ý là RGB(0,32,64); cần golden)** | 0x4063b0, 0x406a90, 0x406e40; ctor 0x405350 |
| -T | không vẽ ảnh, chỉ .txt | driver 0x41f7bc |

### 6.2 Các trường văn bản và nguồn (dùng chung cho _AVE, _6CH, .txt)

Nhãn = `LoadString(conv+0x20 + id)` (conv+0x20 = 0 JP, 10000 EN; bảng chuỗi: scratch/container/strtab.txt). Khoảng trắng chính xác giữa nhãn và giá trị
lấy từ golden.

| Trường | Nguồn trong record (accessor) | Định dạng (formatter) | Chuỗi tài nguyên / hằng |
|---|---|---|---|
| ID | C+0x00 (0x42a6a0) | nguyên văn; **chỉ khi cờ ID** (nhãn luôn có) | 133 "ID:" |
| Tên | C+0x2a (0x42a7e0) | nguyên văn, giữ space cuối; chỉ khi cờ ID | 134 "Name:" |
| Giới tính | C+0x68 (0x42a900) | 0x40d5f0: nhãn + 300/301/302 | 135 "SEX:", 300..302 Male/Female/Unknown |
| Tuổi | C+0x78 (0x40d8a0 → 0x42ba00) | 0x40d6d0: v1 + đơn vị[mã]; mã ≥ 0x10: v1 + đơn vị[k] + v2 + đơn vị[k+1] | 136 "AGE:", 400..404 years/months/weeks/days/hours |
| Chiều cao / cân nặng | C+0x88/+0x8c, C+0x8e/+0x92, cờ C+0x84 | 0x40d8c0 / 0x40d980: nguyên ("* " nếu rỗng) [.lẻ] + "cm"/"inch", "kg"/"pound" | hằng 0x47b438…0x47b44c |
| Huyết áp | C+0x98, C+0x9c, cờ C+0x94 (0x42ada0) | 0x40da40: `s1/s2` + " mmHg"; kPa: atoi×0.1 `%3.1f` + " kpa", rỗng → "*" | 0x47b458, 0x47b450, 0x47b464 |
| Thuốc 1/2 | C+0xb8/+0xbc, C+0xd4/+0xd8 (0x42af00, 0x42afd0) | 0x40d200 (0x40d1a0/0x40d1d0): mã 0..9 → chuỗi 500+mã; −1 → văn bản; rỗng → "None" | 150 "Medication:", 500..509 |
| KTV / BS chỉ định / bác sĩ | C+0x177 / C+0x162 / C+0x18c | nguyên văn | 160 "Tech.", 159 "Req.Doctor", 132 "Doctor:" |
| Ngày ghi | rec+4 (0x426ae0 → 0x429360) | 0x40dc00: giờ < 12 → `%04d/%02d/%02d AM% 2d:%02d`, ngược lại `… PM% 2d:%02d` với giờ − 12; **không in giây**; năm 0 → "" | 130 "Acq. Date:", 0x47b498 / 0x47b478 |
| Ngày sửa | W+0x63d4 (0x428500) | 0x40dc00 | 131 "Edit Date:"; không sửa → 140 "Need overread by doctor." **(đoán thứ tự, theo .txt 0x409269…0x40930e)** |
| Dòng thiết bị | B (0x426b20): +0x20, +8/+0xc, +0x18/+0x1c, +0x10/+0x14 | 0x40c9d0: `%s %02d-%02d` + ` %02d-%02d` + ` %02d-%02d` | 0x47b344, 0x47b334 |
| Loại khám | [rec+0] (0x426aa0); kiểu 2: X (0x4273d0) | chuỗi 100 "Rest"/"安静時"; kiểu 2: 175 "after EXER" + ` M'SS"` + `( a/ b[k])` (0x40d390) | 100, 175; 0x47b3d8…0x47b424 |
| Số đo | M+0x2b8, X7+2…+0xe, M+0xa4, M+0xcc | 6.3 | 151..156, 161 |
| Minnesota | W+0x15e6 (0x428070) | ` (%02d-%02d) ` rồi mỗi mã 0x40cb60 ` %d-%d` / ` %d-%d-%d` | 157 "Minnesota", 0x47b2c8, 0x47b360, 0x47b354 |
| Câu kết luận | W+0x948 dòng i (0x427f50) + mã W+0x914 (0x427ec0) | .txt: `%c%-5s %-s` / dòng tiếp `       %-s` (0x408f30…0x40908c); mã `%-6s` (0x40ceb0); "Ohters  " (sic, 0x47b2dc) | — |
| Câu kết luận không vừa | như trên | Khi tổng số dòng (sau khi ngắt 47 byte) > 12: **.txt** in đủ 12 ô (0x408f75, vòng lặp không so giới hạn); **_AVE** (0x40a7eb) chỉ có 11 dòng: câu cuối + các câu đầu 0,1,… chừng nào tổng còn ≤ 11, rồi một dòng `Ohters  ` + mã của các câu bị bỏ, cách nhau dấu phẩy (0x40aa46). Câu cuối luôn in sau cùng với mã thứ nhất; các câu khác mang mã của câu kế tiếp (chỉ số i+1) — giữ nguyên như bản gốc. | 2026-11-12, gdi_check trên 322 mã |
| Khối nhận xét | W+0x63e8/+0x64e8 (tag 7), W+0x1618+4 | 0x40de80: có ngày sửa → chuỗi dài nếu ≠ "" không thì chuỗi ngắn; không sửa → JP: câu khuyến cáo W+0x1618+4, EN: "" ; ngắt dòng 48 ký tự (0x40e460) | — |
| Nhãn lọc | D+0, D+4, idx bộ lọc thật dùng (−H trước, rồi D+8) | 0x40cfe0: `Filter:(%s %s)%s` = (nguồn, trôi, thông thấp); nguồn 0 "H50", 1 "H60", 2 "Hum OFF", khác "UNKNOWN"; trôi 1 "d", 2 "D", khác ""; thông thấp 0 " 25Hz", 1 " 35Hz", 2 " 75Hz", 3 "100Hz", 4 "150Hz", 5 " OFF" | 0x47b36c…0x47b3cc |
| Nhãn tốc độ | hằng | `10mm/mV 25mm/s` (0x47b320), `10mm/mV 25mm/s Average` (0x47b20c), `Rhythm[%s] 10mm/mV 25mm/s  ` (0x47b2a4, tên từ RhythmLead) | — |

### 6.3 Số đo (formatter dùng chung .txt và _AVE; _6CH chỉ dùng HR 0x40be8d và X7 0x40c025)

| Nhãn (JP / EN id) | Giá trị | Hợp lệ | Dạng | Bằng chứng |
|---|---|---|---|---|
| 151 Heart Rate | M+0x2b8 (s16) | 0 < v < 10000 | `v` hoặc `***`, + " bpm" | 0x40db80 |
| 152 PR Int. / 153 QRS Int. | X7+2 / X7+4 | 0 < v < 10000 | + "  ms" | 0x40df70 / 0x40dff0 |
| 154 QT/QTc Int. | X7+6 "/" X7+8 | 0 < v < 10000 | QTc sai → **cả chuỗi** thành "***"; + "  ms" | 0x40e070, 0x40e100 |
| 155 P/QRS/T Axis | X7+0xa, +0xc, +0xe | −360 ≤ v ≤ 360 | `P/ QRS/ T` (sai: "***"/" ***") + " deg" | 0x40e150 |
| 156 RV5/SV1 Value | M+0xa4 × 0.001 "/" M+0xcc × 0.001 | 0 ≤ v ≤ 0x7fff | `%4.2f` + "  mV"; SV1 sai → cả chuỗi "***  mV" | 0x40e290, 0x40e354 |
| 161 RV5+SV1 Value | (M+0xa4 + M+0xcc) × 0.001 | 0 ≤ tổng ≤ 0x7fff | `%4.2f` + "  mV" | 0x40e3c0 |

Không có block 7/0x110 ⇒ mọi giá trị = 0 ⇒ HR/PR/QRS/QT "***", trục "0/ 0/ 0 deg", RV5/SV1 "0.00/0.00  mV" **(đoán từ điều kiện hợp lệ)**.
Chuỗi tài nguyên 600–606 ("** normal ECG **"…) **không** được EcgPaint nạp (quét immediate; chỉ 0x258 = 600 px ở 0x4079aa/0x407ab7).

### 6.4 File .txt (0x408630) — thứ tự dòng đọc từ code; nội dung byte chính xác lấy từ golden

Mỗi dòng kết thúc `\r\n` (0x47b31c); nhãn từ LoadString.
1. 133 "ID:" [+ ID nếu cờ ID] 2. 134 "Name:" [+ tên nếu cờ ID] 3. giới tính (0x40d5f0) 4. tuổi (0x40d6d0)
5. chiều cao, cân nặng, huyết áp (0x40d8c0, 0x40d980, 0x40da40) 6. 150 "Medication:"; thuốc 1, thuốc 2 (0x40d1a0, 0x40d1d0)
7. số đo 151, 152, 153, 154, 155, 156, 161 (mỗi dòng: nhãn + "  " + giá trị) 8. 160 "Tech." + KTV; 159 "Req.Doctor" + BS chỉ định
9. câu kết luận (mã + văn bản, 0x408ef2…0x4090de) 10. khối nhận xét (0x40de80) 11. ngày sửa / "Need overread by doctor." + 132 "Doctor:" + bác sĩ (0x409269…0x40931f)
12. dòng thiết bị (0x40c9d0) 13. loại khám (100 + 0x40d390) 14. 157 "Minnesota" + ` (%02d-%02d) ` + mã (0x409421…0x4094dd)
15. ngày ghi (0x40dc00) 16. `Rhythm[%s] 10mm/mV 25mm/s  ` + nhãn lọc (0x4095dd, 0x40cfe0).
Ghi bằng CFile của MFC (0x45000c/0x45002d); trả 0 khi lỗi → exit −2 (driver 0x41f943).

### 6.5 Ảnh `_AVE` và `_6CH`: dữ liệu được vẽ (đã biết từ code)

| Trang | Dữ liệu | Thang (đoán, mục 4.2.4) | Bằng chứng |
|---|---|---|---|
| _AVE, nhịp TB | 12 lead, thứ tự hiển thị (bảng 0x46f270), mỗi lead avg[s][x0 .. x1), **không lọc** | 40 LSB/mm, 20 mẫu/mm | 0x4063b0 (gọi từ 0x40b103) |
| _AVE, dải nhịp | lead `RhythmLead` (bảng 0x46f484), min(N, 5000) mẫu đầu, buffer đã lọc, dịch đường đáy wave+0x1a0 | 80 LSB/mm, 20 mẫu/mm | 0x406a90 (gọi từ 0x40b364) |
| _6CH | n = min(N, 2500); lead I..aVF mẫu 0..n−1, V1..V6 mẫu 2500..2500+n−1; buffer đã lọc; dịch đường đáy wave+0x1d0 | 80 LSB/mm | 0x406e40 (gọi từ 0x40bbca), 0x4071c9…0x4071ed |
| điểm ảnh | y = y0 − trunc(v / (LSB/px)) (`_ftol` cắt về 0); mỗi cột pixel vẽ đoạn dọc min..max các mẫu rơi vào cột | — | 0x405650, 0x405490, 0x405570 |

### 6.6 Bố cục ảnh — **CHƯA ĐỌC TĨNH (cố ý)**

Bố cục trang (lề, vị trí từng khung, ô lưới 1 mm / 5 mm theo pixel, vị trí và font của từng chuỗi, độ dày/màu bút, khung 40 mm / 170 mm,
vị trí x0 của mỗi cột, cách áp offset đường đáy) **sẽ được đo từ các lệnh GDI của bản gốc** (`wine_trace.sh -r gdi`: `CreatePen`, `CreateFontA`,
`MoveToEx`/`LineTo`/`Polyline`, `TextOutA`/`ExtTextOutA`, `Rectangle`, `FillRect`) khi chạy trên file tổng hợp của
`research/notes/synthetic-plan.md`, cho cả ba `IMGSIZE`, JP và EN, có và không có ID. Mục này sẽ được viết lại từ trace đó
(dự kiến: một bảng toạ độ theo mm và theo pixel cho mỗi `IMGSIZE`). Các hằng đã biết ở 6.1–6.5 là giả thuyết để trace kiểm tra.

### 6.7 Chỗ bản gốc tự hỏng (ecg-paint không bắt chước) — tìm bằng so sánh ngẫu nhiên 2026-11-12

| Đầu vào | Bản gốc | ecg-paint |
|---|---|---|
| Record < 5000 mẫu (ngắn hơn 10 s) | `_6CH` vẽ vùng nhớ sau buffer mẫu (rác); dưới ~3600 mẫu thì sập sau khi ghi `_AVE` | vẽ đường thẳng, chuyển đổi bình thường |
| Ngày có tháng/ngày/giờ/phút ≥ 100 hoặc năm > 9999 | DateTime giữ chuỗi trong các ô cố định (+0 "%04d", +5/+8 "%02d", giờ ở +0xb…): chuỗi dài tràn sang ô kế, getter strcpy (0x429030/0x428c50) lại tràn buffer stack của nơi gọi ⇒ "1899/202100/193", tên file "18992021193000000"; năm 0xFFFF in "-001" | in đúng các số |
| `RhythmLead` không phải tên chuyển đạo | getRhythmLead trả −1, EXE dùng chỉ số −1 (bảng 0x46f484, dữ liệu sóng) ⇒ vẽ vùng nhớ khác | vẽ II |
| Mã chữ 2 (header +0x0f), cờ `-H-1` | sập (0xC0000005), không ghi file | thoát −2 |
| Sóng kiểu 5 với 12000 mẫu | 2/4 ca ngẫu nhiên: một ca sập, một ca hai nhãn (id 160, 161) thành rác (cả trên Windows lẫn Wine); kiểu 5 với ≤ 9000 mẫu đều khớp | chuyển đổi bình thường |
| ID có byte tiếng Nhật và ID nằm trong tên file | byte đi thẳng vào API file ANSI ⇒ tên file tuỳ code page của Windows | tên như trên Windows tiếng Nhật (giải mã Shift-JIS) |


## 7. Câu hỏi mở

Định dạng / thiết bị (chỉ mẫu thật trả lời):
- Thiết bị gửi tên/đuôi file gì, một hay nhiều file mỗi lần đo? Biến thể nào (0 nghỉ / 0xffff sau gắng sức) xuất hiện thực tế?
- Máy ghi mã chữ 0 (EUC-JP) hay 1 (Shift-JIS)? Trường văn bản đệm space hay NUL (tên không trim nên space cuối sẽ lên ảnh/.txt)?
- Máy ghi block 6 hay block 0x100? (quyết định nguồn bộ lọc thông thấp khi không có `-H` — Bridge không truyền `-H`.)
- Máy dùng method nén nào (B8+0xe)? Từ đó biết phần nào của mục 4.2.2 phải chạy đúng trước.
- Khối nén 1..8 có đúng thứ tự I, II, V1..V6 không (với I/II chắc chắn nhờ công thức dẫn xuất)?
- N thực tế = 5000 (10 s)? Nếu N < 5000 trang _6CH đọc quá buffer ở cột ngực (bản Rust cần quyết định: 0 hay lặp hành vi).
- Giá trị x0/x1 điển hình của cửa sổ nhịp TB.
- Header +0x0e (0/1); B1+0x56; B+0x60/+0x64; nhiều trường C chỉ writer dùng (C+0x15, +0x3f, +0xa0, +0xa4, +0xef, +0x106, +0x11d, +0x134, +0x14b, +0x1a1,
  +0x1ac, +0x1d5, +0x1fe); X+4, X+0x24; X7+0x10..+0x16, W+0x8dc; A2 của tag 8; danh sách A/B của tag 0x113; 28/26 mảng của tag 0x110 (ngoài R/S);
  ý nghĩa cờ word 1..3 (method 16) và byte 1 = 0x80 (danh sách 2). Không ảnh hưởng đầu ra của EcgPaint.
- Nghĩa của hai số trong `( a/ b[k])` (dòng sau gắng sức).

Đơn vị / bố cục (golden từ file tổng hợp trả lời được):
- 500 Hz, 1.25 µV/LSB (nhịp), 2.5 µV/LSB (nhịp TB): cần xác nhận bằng xung 1 mV × 200 ms (synthetic-plan.md).
- Toàn bộ bố cục ảnh (6.6); bút sóng đen 1 px hay màu khác; khoảng trắng chính xác của .txt và nhãn; cách áp offset đường đáy và kẹp.
- `CreateCompatibleBitmap` trên DC "DISPLAY": độ sâu màu của ảnh ra (24/32 bpp?) trên Windows và dưới Wine; dưới Wine không có display thì sao.
- Bridge Linux đọc `/tmp/EcgViewer/fileout`, còn EcgPaint dưới Wine ghi vào `%TEMP%` của prefix: kiểm chứng bằng một lần chạy thật.

Hành vi lỗi / môi trường:
- Trên Windows thật, file hỏng → crash use-after-free (WER?) hay thoát −2?
- Thiếu `t_ecaps12c.dll` → crash lúc khởi động (gọi con trỏ NULL, 0x403d4c)?

Bản Rust:
- Bảng câu kết luận/khuyến cáo là dữ liệu của vendor trong `t_ecaps12c.dll`: đọc từ DLL lúc chạy (DLL vẫn có trong thư mục của Bridge) hay nhúng?
  Cần người dùng quyết định (bản quyền).
- Bộ giải nén Python mới chỉ được kiểm với bộ nén viết từ cùng mô tả: cần đối chiếu với DLL thật (file tổng hợp method 1/4/16 qua bản gốc, so ảnh).

## 8. Mẫu

Chưa có mẫu thật, chưa có golden (`tests/golden/` trống). Kế hoạch file tổng hợp và cách lấy golden: `research/notes/synthetic-plan.md`
(sẽ hiện thực thành `research/py/mkecg.py`). File tổng hợp cũ của chủ đề waveblocks (chưa chạy qua bản gốc):
`research/scratch/waveblocks/v2/synth_rest_m0.ecg` (method 0) và `.ecg.m1` (method 1).

| Case | Nguồn | Đầu ra gốc | Ghi chú |
|---|---|---|---|
| (chưa có) | | | |

## Phụ lục A. Mâu thuẫn giữa các ghi chú và cách giải quyết (kiểm lại trong code khi hợp nhất, 2026-11-04)

| # | Mâu thuẫn | Kết luận | Bằng chứng kiểm lại |
|---|---|---|---|
| 1 | NOTES.md (và tóm tắt container) nói vùng 0x410ebb… của parse là "giải nén"; waveblocks nói đó là GetAnalText, giải nén nằm trong handler tag 8 | **waveblocks đúng** | 0x411032 `call 0x404170` (wrapper GetAnalText); 0x41d925 `call 0x404380` (wrapper Decomp12lead) trên object tĩnh 0x47f5b8 (0x41d920) |
| 2 | Bộ chọn ngôn ngữ của GetAnalText: decomp suy "JP → sel 0" chỉ dựa vào tên đối số trong NOTES | **đúng, đã kiểm** | driver 0x41f76a…0x41f788 truyền Init(res=[0x5455dc], jp=[0x5455e0], density=[0x5455e4]); [0x5455e0] = (getJapanese ≠ 0) ở 0x41f496…0x41f4a1; Init 0x407963…0x407970: jp = 0 → conv+0x20 = 10000; sel = (conv+0x20 ≠ 0) (0x410f1c…0x410f24) |
| 3 | Biên độ: decomp ví dụ "TB 5 µV/LSB, nhịp 2.5 µV/LSB"; waveblocks "TB 2.5, nhịp 1.25 µV/LSB" | ví dụ của decomp không phải bằng chứng; dùng **giá trị waveblocks** (suy từ hằng vẽ + nhãn 10mm/mV), vẫn **(đoán)** tới khi có golden | 0x46f3f8 = 20.0, 0x46f458 = 4.0, `fadd st0,st0` 0x4064bc (đọc lại bằng pefile) |
| 4 | Container: bug `%03.3d` với địa chỉ chuỗi tuổi "vô hại vì Analysis là stub" | vô hại nhưng **vì lý do khác**: struct 0x100 byte đi vào GetAnalText (meas+0xde6) và chỉ được dùng cho phần chi tiết `::[…]` mà EcgPaint tắt | 0x410f35 `call 0x411990`, 0x410f3c `mov [ebx], ebp` (arg+0 của GetAnalText); notes/decomp.md §11.4 |
| 5 | waveblocks lần 1 cho rằng file vFFFF chạy với record type 0 | đã được waveblocks lần 2 tự sửa; khớp container: vFFFF → record kiểu 2 (type 0xffff) | 0x410d26…0x410d73 (`cmp eax, edi` với edi = 0xffff) |
| 6 | NOTES.md: "aVR = −(I+II)/2, aVL = I − II/2…" (chia); waveblocks/decomp: dịch số học, hai công thức aVR khác nhau | công thức **dịch số học**; EXE: −((I+II)>>1), DLL: (−(I+II))>>1 | EXE 0x405c5f…0x405c82 (`sar edi,1; neg edi`; `sar di,1`; `sar cx,1`); DLL 0x100042e7… |
| 7 | NOTES.md câu hỏi "EN có in câu tiếng Nhật không?" | không: DLL có bảng EN đầy đủ (sel 1) | DLL 0x10008024; notes/decomp.md §11.2 |
| 8 | Tag 8: loại nén → method (waveblocks) và method của DLL (decomp) | khớp: t = 1 → 1, 2 → 4, 3 → 5, 4 → 16, khác → 0 | bảng 0x41d9f0 = {0x41d86d, 0x41d8da, 0x41d8e8, 0x41d8cc} (đọc lại); 0x41d860 `dec eax; cmp eax,3; ja` → mặc định ebp = 0 |
| 9 | Tên đối tượng record: container gọi rec+0x2ac là D và rec+0x5e840 là G; waveblocks gọi chúng là F và W, và gọi rec+0x5e834 là W2; waveblocks dùng "X" cho khoảng/trục tag 7 (W+0x614c) trùng tên với X gắng sức (rec+0x654fc) của container | không phải mâu thuẫn về dữ liệu, chỉ khác tên; tài liệu này dùng D, F, W, X (gắng sức), X7 (tag 7) theo bảng đầu file | offset kiểm qua getter 0x426e40/0x426e80/0x426ec0/0x426f00 và 0x4273d0 |
| 10 | Container đánh dấu "không chắc" thứ tự 12 cặp theo chuyển đạo của block 6 (bảng 0x46f678) | bảng 0x46f678 − 1 = {0,1,8,9,10,11,2..7} = đúng bảng hiển thị→lưu trữ 0x46f270/0x46f484 ⇒ 12 cặp trong file theo **thứ tự hiển thị** I..aVF, V1..V6; không ảnh hưởng đầu ra (H không được dùng) | dump dd 0x46f678, 0x46f270, 0x46f484, 0x46f2a0 (scratch/spec/tblchk.sh) |

Không giải quyết được bằng phân tích tĩnh: các mục ở §7 (đơn vị tuyệt đối, bố cục, ý nghĩa trường, hành vi trên Windows thật).
