# Tổng quan cập nhật xử lý Audit (Audit Remediation Overview)

- **Ngày thực hiện:** 27/10/2026
- **Tài liệu gốc:** `audit/AUDIT-2026-10-15.md` & `audit/AUDIT-2026-10-15-QUICK.md`
- **Phạm vi cập nhật:** 29 tệp trên toàn bộ workspace (`+506 / -168` dòng)

---

## 1. Tóm tắt kết quả

Toàn bộ các mục lỗi kỹ thuật, bảo mật, hiệu năng, UI/UX và CI/CD được nêu trong báo cáo Audit ngày 15/10/2026 đã được rà soát và xử lý:

| Nhóm mức độ | Mã mục đã xử lý trong đợt cập nhật này | Mã mục đã được xử lý trước đó trên nhánh `main` |
|---|---|---|
| **High & New (`H`, `N`)** | `N1`, `N2`, `N3`, `H3`, `H5` | `H1`, `H2`, `H4`, `H6`, `N4` |
| **Medium (`M`)** | `M1`, `M2`, `M3`, `M4`, `M5`, `M6`, `M7`, `M8`, `M9`, `M10`, `M13` | `M15`, `M16` |
| **Low & Hygiene (`L`)** | `L1`, `L6`, `L8`, `L9`, `L13`, `L14`, `L16`, `L17`, `L20`, `11Oct#21` | `L18` |

---

## 2. Chi tiết các cập nhật theo từng phân hệ (Crates & Hệ thống)

### 2.1. Phân hệ cửa sổ & giao diện (`src/crates/chillstudio`)

#### **M1 & L1 — Sửa tỷ lệ giao diện (`Interface scale`) và loại bỏ `unsafe { std::env::set_var }` lúc chạy**
- **Tệp cập nhật:**
  - `src/crates/chillstudio/src/platform.rs`
  - `src/crates/chillstudio/src/lib.rs`
- **Vấn đề cũ:**
  - Khi người dùng chọn tỷ lệ giao diện (`75%`, `100%`, `125%`, `150%`), ứng dụng gán biến môi trường `SLINT_SCALE_FACTOR = percent / 100.0`. Backend `winit` của Slint coi giá trị này là hệ số tuyệt đối (thay thế hoàn toàn DPI gốc của màn hình) và khóa luôn sự kiện đổi màn hình. Trên màn hình Windows 150% hoặc MacBook Retina (200%), chọn `100%` hoặc `125%` làm giao diện bị thu nhỏ bất thường.
  - Ngoài ra, gọi `unsafe { std::env::set_var("SLINT_SCALE_FACTOR", ...) }` khi các luồng giải mã FFmpeg/âm thanh/xuất video đang chạy vi phạm điều kiện an toàn bộ nhớ đa luồng trên Linux/Android (`getenv` chạy đồng thời).
- **Giải pháp đã áp dụng:**
  - Xóa hoàn toàn hai khối `unsafe { std::env::set_var("SLINT_SCALE_FACTOR", ...) }` trong `src/crates/chillstudio/src/lib.rs`.
  - Bổ sung hàm `platform::apply_ui_scale(window: &slint::Window, percent: Option<u16>)` lấy hệ số DPI gốc của màn hình (`winit.scale_factor()`) nhân với hệ số người dùng chọn (`system * scale_factor(percent)`), sau đó phát sự kiện `WindowEvent::ScaleFactorChanged` và `WindowEvent::Resized`.
  - Lắng nghe `WindowEvent::ScaleFactorChanged` trong `DropHandler` (`src/crates/chillstudio/src/platform.rs`) để tự động tính lại tỷ lệ khi người dùng kéo cửa sổ sang màn hình có DPI khác.

#### **M2 — Vô hiệu hóa các chuẩn nén (codec) không được thiết bị hỗ trợ trên hộp thoại Export**
- **Tệp cập nhật:**
  - `src/crates/chillstudio/ui/dialogs/export.slint`
- **Vấn đề cũ:**
  - Hai điều khiển `Select` chọn codec (cho SDR và HDR) không truyền thuộc tính `disabled`, khiến người dùng vẫn bấm chọn được `AV1` hoặc `HEVC` trên thiết bị không có bộ mã hóa tương ứng (giao diện hiển thị `AV1` nhưng bên dưới xuất ra `H.264`).
- **Giải pháp đã áp dụng:**
  - Truyền `disabled: [!root.data.h264-encodable, !root.data.hevc-encodable, !root.data.av1-encodable]` cho bộ chọn SDR và `disabled: [!root.data.hevc-encodable, !root.data.av1-encodable]` cho bộ chọn HDR.

#### **M3 — Đưa thông báo nổi (`ToastLayer`) vào trong vùng an toàn (`safe-area-insets`) trên điện thoại**
- **Tệp cập nhật:**
  - `src/crates/chillstudio/ui/primitives/toast.slint`
  - `src/crates/chillstudio/ui/app.slint`
- **Vấn đề cũ:**
  - `ToastLayer` đặt thông báo cố định cách cạnh dưới và cạnh phải `16px` mà không trừ phần lề của thanh điều hướng/tai thỏ trên Android (targetSdk 35 edge-to-edge) và iPhone.
- **Giải pháp đã áp dụng:**
  - Bổ sung các thuộc tính `inset-bottom`, `inset-left`, `inset-right` vào `ToastLayer` và nối với `root.safe-area-insets` trong `app.slint`.

#### **M4 — Gom thao tác nhân bản clip (`Duplicate`) thành một bước hoàn tác (Undo) duy nhất**
- **Tệp cập nhật:**
  - `src/crates/chillstudio/src/studio.rs`
- **Vấn đề cũ:**
  - `Studio::duplicate` gọi `place_copy(..., None)` không có tên gesture, làm lệnh `AddClip` (đặt toàn bộ tệp gốc vào cuối làn) và lệnh cắt/định hình clip thành 2 bước Undo rời nhau. Bấm `Ctrl+Z` một lần sau khi nhân bản để lại cả tệp video gốc dài hàng giờ trên timeline.
- **Giải pháp đã áp dụng:**
  - Trong `Studio::duplicate_selected` và `Studio::duplicate`, gọi `self.flush_commit()`, truyền gesture `Some("duplicate")` vào `self.place_copy(...)`, và đóng gesture bằng `session.end_gesture()` ngay sau khi hoàn tất.

#### **M5 — Khôi phục nối clip không mất chất lượng (`MergeClips`) trước khi render hợp nhất (`Consolidation`)**
- **Tệp cập nhật:**
  - `src/crates/chillstudio/src/studio/consolidate.rs`
- **Vấn đề cũ:**
  - Mọi thao tác `Merge` đều đi qua `Consolidation` (mã hóa lại thành tệp `media/merged-*/merged.mp4` ở CRF 16), kể cả khi người dùng chỉ nối lại hai nửa của cùng một clip vừa cắt (`Split`).
- **Giải pháp đã áp dụng:**
  - Kiểm tra `chillstudio_project::why_not_merge(self.timeline(), &self.selection).is_none()` trong `merge_blocked()` và `merge()`. Khi các clip được chọn liền kề và liên tục từ cùng một tệp nguồn, thực thi ngay `Command::MergeClips` tức thì và không giảm chất lượng; chỉ khi không thỏa điều kiện nối nguyên bản mới chuyển sang render `Consolidation`.

#### **M8 — Đồng bộ âm thanh chuyển cảnh (crossfade) khi phát thử trong cửa sổ dựng**
- **Tệp cập nhật:**
  - `src/crates/chillstudio/src/studio.rs`
  - `src/crates/chillstudio-export/src/lib.rs`
- **Vấn đề cũ:**
  - Luồng phát âm thanh trong trình dựng (`Studio::sync_audio`) lấy trực tiếp `session.flattened_clips()` chưa qua bước giải quyết chuyển cảnh (`resolve_transitions`), dẫn tới trong trình dựng nghe tiếng cắt đột ngột còn tệp xuất ra lại crossfade và kéo dài âm thanh qua điểm cắt.
- **Giải pháp đã áp dụng:**
  - Cung cấp hàm công khai `chillstudio_export::apply_transitions(&mut clips, rate)` và gọi trong `Studio::sync_audio` trước khi chuyển thành `audio_specs(&clips)`.

#### **L6 — Chống đóng nhầm hộp thoại Captions do thông báo trễ từ tiến trình đã hủy**
- **Tệp cập nhật:**
  - `src/crates/chillstudio/src/panes/captions.rs`
- **Vấn đề cũ:**
  - `CaptionsMsg::Progress` và `CaptionsMsg::Finished` không gắn mã phiên chạy (`run_id`). Nếu người dùng bấm `Cancel` rồi mở lại hộp thoại Captions ngay, thông báo kết thúc trễ (`Err("...cancel...")`) của phiên cũ sẽ đóng hộp thoại mới mở.
- **Giải pháp đã áp dụng:**
  - Bổ sung bộ đếm `run_id: u64` vào `CaptionsPane` (giữ nguyên khi `open_on`, tăng khi `Begin` hoặc `Cancel`), gắn `run_id` vào `CaptionsMsg::Progress(u64, JobStage)` và `CaptionsMsg::Finished(u64, Result<Vec<Segment>, String>)`, và bỏ qua mọi thông điệp có `run_id != self.run_id`.

---

### 2.2. Phân hệ kết xuất & chuyển cảnh (`src/crates/chillstudio-export`)

#### **N2 — Sửa lỗi chuỗi chuyển cảnh liên tiếp (`A → B → C`) leo sang làn của track phía trên**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-export/src/lib.rs`
  - `src/crates/chillstudio-export/src/resolve.rs`
- **Vấn đề cũ:**
  - Khi có chuỗi chuyển cảnh `A → B → C` trên cùng track `t` (làn gốc `2*t`), hàm `overlap` đọc `a.track` tại thời điểm giải quyết và đặt `b.track = a.track + 1`. Vì `B` đã được đẩy lên `2*t + 1`, `C` bị đẩy tiếp lên `2*t + 2` — tức là làn chính của track `t + 1` (ví dụ track phụ đề/title), làm mất hình của một trong hai clip khi chồng lấp.
- **Giải pháp đã áp dụng:**
  - Sắp xếp danh sách `cuts` trong `resolve_transitions_at` theo thời điểm bắt đầu của clip đi vào (`clips[cut.incoming].start`) để giải quyết tuần tự từ trái sang phải.
  - Trong `overlap`, tính làn gốc của track bằng `let base = a.track & !1;`, luôn đặt clip đi vào ở sub-lane chuyển cảnh `b.track = base + 1;` và đặt khung hình giữ (`hold`) ở `hold.track = base;`.
  - Trong `resolve::build_timeline`, sắp xếp các clip trong cùng một làn theo `(clip.start, !clip.hold)` và cấp phát một `TrackId` mới trong `Timeline` khi hai clip trên cùng sub-lane `2*t + 1` chồng lấp thời gian, đồng thời vẫn ghi nhận `tracks_of.insert(id, clip.track)`.
  - Trong `transition_stages`, dùng `rposition(|layer| layer.track == span.to_track)` để tách đúng lớp đi vào mới nhất làm `to`, giữ lớp đi ra trên cùng sub-lane trong `from`.
  - Bổ sung unit test `a_chain_of_transitions_stays_on_its_own_track` kiểm chứng chuỗi 4 clip có chuyển cảnh liên tiếp.

#### **L8 — Chuyển tiếp `lost()` và `deliver_hdr()` trên `EffectClock`**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-export/src/lib.rs`
- **Giải pháp đã áp dụng:**
  - Bổ sung chuyển tiếp `fn lost(&self) -> bool` và `fn deliver_hdr(&mut self, hdr: bool)` từ `EffectClock` xuống `self.inner` trong `impl Compositor for EffectClock<'_>`.

---

### 2.3. Phân hệ đa phương tiện & FFmpeg (`src/crates/chillstudio-media`)

#### **N1 — Kích hoạt giải mã phần cứng MediaCodec trên Android**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-media/src/hardware.rs`
  - `src/crates/chillstudio-media/src/decode.rs`
- **Vấn đề cũ:**
  - `Decoder::open_codec` mở decoder mặc định của FFmpeg (`ffmpeg::decoder::find(context.id())`). Trong FFmpeg, các bộ giải mã mặc định `h264`, `hevc`, `vp8`, `vp9`, `av1` không đăng ký `AVCodecHWConfig` cho MediaCodec; MediaCodec chỉ tồn tại dưới dạng các decoder độc lập `h264_mediacodec`, `hevc_mediacodec`, `vp8_mediacodec`, `vp9_mediacodec`, `av1_mediacodec`. Hệ quả là mọi video trên Android đều bị giải mã bằng CPU.
- **Giải pháp đã áp dụng:**
  - Thêm phương thức `Device::decoder_for(&self, id: ffmpeg::codec::Id) -> Option<ffmpeg::Codec>` trong `hardware.rs` để tìm đúng decoder `*_mediacodec` theo tên khi `self.kind == HwDevice::MediaCodec`.
  - Cập nhật `Decoder::open_codec` và `Decoder::next_source` trong `decode.rs` để mở `context.decoder().open_as(codec)` ở chế độ buffer (`Pixel::NONE`) khi chạy trên MediaCodec, tự động quay về phần mềm nếu mở thất bại.

#### **L9 — Không làm rơi gói tin (`Packet`) khi bộ giải mã trả về `EAGAIN`**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-media/src/decode.rs`
  - `src/crates/chillstudio-media/src/audio.rs`
  - `src/crates/chillstudio-media/src/samples.rs`
- **Vấn đề cũ:**
  - Khi `decoder.send_packet(&packet)` trả về `EAGAIN` (hàng đợi đầu ra của decoder đã đầy — rất phổ biến với `*_mediacodec`), gói tin vừa đọc từ demuxer bị bỏ qua mà không được gửi lại sau khi rút khung hình ra, gây vỡ hình/mất mẫu âm thanh.
- **Giải pháp đã áp dụng:**
  - Thêm trường `pending_packet: Option<ffmpeg::Packet>` vào cả 3 bộ giải mã (`Decoder`, `MixInput`, `AudioDecoder`). Khi `send_packet` trả về `EAGAIN`, gói tin được cất vào `pending_packet` và gửi lại ngay ở vòng lặp tiếp theo sau khi `receive_frame` đã rút bớt khung hình; xóa `pending_packet` khi `jump` hoặc `resume_in_software`.

#### **M7 — Cho phép hủy (`Cancel`) ngay trong quá trình trộn âm thanh xuất video**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-media/src/audio.rs`
  - `src/crates/chillstudio-export/src/lib.rs`
- **Giải pháp đã áp dụng:**
  - Thêm hàm `audio::mix_to_file_cancellable` nhận `cancel: &AtomicBool`, kiểm tra `cancel.load(Ordering::Relaxed)` trong vòng lặp `'mix` và trả về `Error::Cancelled`. Cập nhật `render_on` gọi `mix_to_file_cancellable` với `reporter.cancel`.

#### **M9 — Ghi nhận các dòng lỗi `AV_LOG_ERROR` từ FFmpeg vào nhật ký ứng dụng**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-media/src/ffi.rs`
- **Giải pháp đã áp dụng:**
  - Trong `av_log_callback`, ghi các thông điệp ở mức `AV_LOG_ERROR` (trừ dòng nhiễu `"mmco: unref short failure"` của H.264) qua `log::warn!(target: "ffmpeg", "{text}")` để nguyên nhân lỗi mở bộ mã hóa/giải mã (MediaCodec, NVENC, x264) hiển thị rõ trong tệp log.

---

### 2.4. Phân hệ nền tảng máy chủ & dịch vụ (`src/crates/chillstudio-host`)

#### **N3 — Tắt mã hóa phần cứng ngầm khi tạo proxy cho clip HDR**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-host/src/proxy.rs`
- **Giải pháp đã áp dụng:**
  - Đặt `hardware: false` khi tạo proxy nền để việc nhập (import) một tệp 4K HDR không tự ý gọi `hevc_nvenc` ngầm trên các laptop hybrid GPU (tránh lỗi segfault trong `libcuda`) và không bị bộ lọc 10-bit chặn trên Android.

#### **M13 & L13 — Đảm bảo an toàn tín hiệu (async-signal-safety) và không cấp phát bộ nhớ trong Crash Handler**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-host/src/crash.rs`
- **Giải pháp đã áp dụng:**
  - Trên Unix (`where_`), thay `CStr::from_ptr(info.dli_fname).to_string_lossy()` (có thể cấp phát `String` trên heap nếu tên đường dẫn không phải UTF-8) bằng `to_str().unwrap_or("?")` và cập nhật chú thích `SAFETY` của `dladdr`.
  - Trên Windows (`describe`), thay `std::thread::current().name()` (có thể cấp phát bộ nhớ hoặc `abort` nếu lỗi xảy ra trên luồng C/driver/FFmpeg hoặc sau khi TLS đã hủy) bằng `GetCurrentThreadId()`.
  - Loại bỏ 2 mã ngoại lệ fail-fast `0xC000_0409` (`STATUS_STACK_BUFFER_OVERRUN`) và `0xC000_0374` (`STATUS_HEAP_CORRUPTION`) khỏi bảng `CODES` vì Windows kết thúc tiến trình ngay lập tức mà không bao giờ gọi `UnhandledExceptionFilter`.

#### **L14 — Gộp trung bình hai kênh âm thanh khi phát ra thiết bị Mono (1 kênh)**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-host/src/playback.rs`
- **Giải pháp đã áp dụng:**
  - Khi thiết bị đầu ra có `channels == 1`, gán `frame[0] = (left + right) * 0.5` thay vì chỉ lấy kênh trái và bỏ mất kênh phải.

---

### 2.5. Phân hệ Remote API (`src/crates/chillstudio-server` & `src/crates/chillstudio-api`)

#### **M6 — Khắc phục lỗ hổng cạn kiệt File Descriptor qua kết nối chưa xác thực**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-server/src/lib.rs`
  - `src/crates/chillstudio-server/src/json.rs`
- **Vấn đề cũ:**
  - Mỗi kết nối TCP/Unix đến đều được nhân bản (`try_clone`) và đẩy một closure đóng socket vào `Vec` trong `Connections`, nhưng không bao giờ được gỡ ra khi kết nối đóng hoặc xác thực thất bại, gây rò rỉ file descriptor cho tới khi chạm giới hạn hệ điều hành (`EMFILE`).
- **Giải pháp đã áp dụng:**
  - Đổi `Connections` sang `Arc<Mutex<HashMap<u64, Box<dyn Fn() + Send>>>>`. Mỗi kết nối được cấp một `id: u64` tăng dần và tự động gỡ khỏi `connections` ngay khi luồng kết nối kết thúc (hoặc khi `Builder::new().spawn` thất bại).
  - Thêm khoảng nghỉ `10ms` khi `listener.accept()` gặp lỗi để tránh vòng lặp quay 100% CPU nếu hệ thống hết file descriptor.

#### **11Oct#21 — Cập nhật tài liệu crate `chillstudio-api`**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-api/src/lib.rs`
- **Giải pháp đã áp dụng:**
  - Thay cụm `"an MCP server"` trong doc comment đầu crate bằng `"an agent or script"`.

---

### 2.6. Hiệu ứng WGSL, Android, iOS, Đa ngôn ngữ & CI Workflows

#### **L16 — Tránh gọi `pow(x, 2.0)` với cơ số âm trong shader `chillstudio.light-leak-flash`**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-effects/packages/chillstudio.light-leak-flash/effect.wgsl`
- **Giải pháp đã áp dụng:**
  - Trong chuẩn WGSL/HLSL/MSL, `pow(x, y)` với `x < 0` có kết quả không xác định (`NaN`). Thay `pow(leak_pos - progress, 2.0)` bằng `let d = leak_pos - progress; let leak = exp(-d * d * 12.0);`.

#### **H5 & M10 — Bảo vệ vòng đời Activity và trình chọn tệp trên Android**
- **Tệp cập nhật:**
  - `src/crates/chillstudio-android/src/lib.rs`
  - `src/crates/chillstudio-android/java/ChillStudioFiles.java`
- **Giải pháp đã áp dụng:**
  - Dùng `std::sync::Once` trong `android_main` (`lib.rs`) để đảm bảo `activity::name_directories` và `activity::open_log` chỉ chạy đúng một lần duy nhất trong suốt vòng đời tiến trình, và thoát sớm an toàn nếu `android_main` bị gọi lại.
  - Trong `ChillStudioFiles.java`, bắt `Exception` (thay vì chỉ `IOException`) trong `copy()` để không làm sập ứng dụng khi ContentProvider ném `SecurityException`, bổ sung fallback `activity.getFilesDir()` nếu `getExternalFilesDir(null)` trả về `null`, và bọc `filesPicked(...)` trong khối `try / finally` để callback Rust luôn nhận được phản hồi.

#### **H3 — Bật quyền truy cập thư mục ứng dụng trong ứng dụng Files trên iOS**
- **Tệp cập nhật:**
  - `src/scripts/ios-app.sh`
- **Giải pháp đã áp dụng:**
  - Thêm `<key>UIFileSharingEnabled</key><true/>` và `<key>LSSupportsOpeningDocumentsInPlace</key><true/>` vào `Info.plist` khi đóng gói `.app` cho iOS.

#### **L20 — Bắt lỗi khi tệp ngôn ngữ bị thiếu khóa dịch (`missing`) trong `scripts/locales.py`**
- **Tệp cập nhật:**
  - `scripts/locales.py`
- **Giải pháp đã áp dụng:**
  - Đặt `failed = 1` trong hàm `check()` khi phát hiện bất kỳ khóa nào trong `missing`.

#### **L17 — Khai báo `permissions: contents: read` cho các GitHub Actions workflows**
- **Tệp cập nhật:**
  - `.github/workflows/ci.yml`
  - `.github/workflows/mobile.yml`
  - `.github/workflows/build-app.yml`
- **Giải pháp đã áp dụng:**
  - Thêm khối `permissions: contents: read` ở mức top-level cho cả 3 tệp workflow theo nguyên tắc đặc quyền tối thiểu.

---

## 3. Danh sách toàn bộ 29 tệp đã cập nhật

```text
 .github/workflows/build-app.yml                                          |  3 +
 .github/workflows/ci.yml                                                 |  3 +
 .github/workflows/mobile.yml                                             |  3 +
 scripts/locales.py                                                       |  1 +
 src/crates/chillstudio-android/java/ChillStudioFiles.java                | 24 ++++--
 src/crates/chillstudio-android/src/lib.rs                                | 15 +++-
 src/crates/chillstudio-api/src/lib.rs                                    |  4 +-
 src/crates/chillstudio-effects/packages/chillstudio.light-leak-flash/... |  3 +-
 src/crates/chillstudio-export/src/lib.rs                                 | 92 +++++++++++++++++++---
 src/crates/chillstudio-export/src/resolve.rs                             | 38 +++++++--
 src/crates/chillstudio-host/src/crash.rs                                 | 26 +++---
 src/crates/chillstudio-host/src/playback.rs                              |  4 +-
 src/crates/chillstudio-host/src/proxy.rs                                 |  7 +-
 src/crates/chillstudio-media/src/audio.rs                                | 37 ++++++++-
 src/crates/chillstudio-media/src/decode.rs                               | 70 ++++++++++++----
 src/crates/chillstudio-media/src/ffi.rs                                  |  2 +
 src/crates/chillstudio-media/src/hardware.rs                             | 17 ++++
 src/crates/chillstudio-media/src/samples.rs                              | 17 +++-
 src/crates/chillstudio-server/src/json.rs                                | 86 +++++++++++++-------
 src/crates/chillstudio-server/src/lib.rs                                 |  6 +-
 src/crates/chillstudio/src/lib.rs                                        | 66 ++--------------
 src/crates/chillstudio/src/panes/captions.rs                             | 28 +++++--
 src/crates/chillstudio/src/platform.rs                                   | 51 ++++++++++++
 src/crates/chillstudio/src/studio.rs                                     | 26 +++++-
 src/crates/chillstudio/src/studio/consolidate.rs                         | 16 +++-
 src/crates/chillstudio/ui/app.slint                                      |  3 +
 src/crates/chillstudio/ui/dialogs/export.slint                           |  9 +++
 src/crates/chillstudio/ui/primitives/toast.slint                         | 15 ++--
 src/scripts/ios-app.sh                                                   |  2 +
 29 files changed, 506 insertions(+), 168 deletions(-)
```
