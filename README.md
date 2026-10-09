# Texture Optimizer

Desktop app (Windows / macOS / Linux) tối ưu texture cho Unity, Unreal và Godot. Xây bằng **Tauri 2 + React 19 + TypeScript** (giao diện) và **Rust** (xử lý ảnh, atlas, 3D).

## Tính năng

Mỗi công cụ là một tab; chỉ tab đang mở được nạp, các tab khác "ngủ" và backend giải phóng bộ nhớ ảnh của chúng.

| Tab | Chức năng chính |
|---|---|
| Resize | Up/down scale theo %, kích thước chính xác, fit width/height, cạnh dài nhất; filter Nearest → Lanczos3; resample trong linear space; snap ×4/POT |
| Resolution Fixer | Snap kích thước về bội số 4, bội số N hoặc POT bằng resample / pad / crop, 9 vị trí neo |
| Sprite Trimmer | Cắt pixel trong suốt thừa (ngưỡng alpha, margin, từng cạnh), xuất JSON offset để giữ pivot |
| POT Padding | Thêm padding lên POT (next / square / cố định), nền trong suốt / màu / kéo dài mép |
| Smart Atlas | Gom sprite thành atlas (MaxRects / Skyline), incremental (thêm ảnh thì cập nhật atlas cũ, giữ vị trí sprite cũ), xuất 1 trong 4 định dạng: Generic JSON, Unity (`.meta`, giữ GUID), Godot 3/4 (`.tres`), Unreal Paper2D |
| Pattern Renamer | Đổi tên theo template (`{name}`, `{index}`, `{type}`…), prefix/suffix, đánh số, đổi kiểu chữ, find/replace regex, quy ước tên theo engine; xem trước, phát hiện xung đột, hoàn tác (revert) |
| Background Remover | Xóa nền trắng / caro / màu tùy chọn (có công cụ hút màu), flood fill hoặc toàn ảnh, feather, khử viền |
| 3D Texture Packer | Gộp texture của nhiều model `.fbx/.obj/.dae` thành atlas POT (mọi kênh dùng chung layout), remap UV và ghi lại model; với FBX không ghi lại an toàn thì chuyển sang chế độ *UV Remap Data* (JSON + script Unity) |

Dùng chung cho mọi tab: kéo-thả file/thư mục, lưới ảnh 3 kích thước có nút xóa trên từng ô, undo/redo (Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z), preset tham số, xem trước trước/sau, hai ngôn ngữ English / Tiếng Việt.

## Yêu cầu môi trường

- Node.js 24+, Rust stable (1.90+)
- Các phụ thuộc hệ thống của Tauri 2: <https://v2.tauri.app/start/prerequisites/> (Windows: MSVC Build Tools + WebView2; Linux: `libwebkit2gtk-4.1-dev` …)
- Lần build đầu tải bản Assimp dựng sẵn (~21 MB, crate `asset-importer-sys`), **không cần CMake**. Build offline: đặt `ASSET_IMPORTER_PACKAGE_DIR` hoặc `ASSET_IMPORTER_CACHE_DIR`. Xem [docs/spikes/3d-assimp.md](docs/spikes/3d-assimp.md).

## Lệnh thường dùng

```bash
npm ci                 # cài phụ thuộc frontend
npm run tauri dev      # chạy app ở chế độ dev
npm run tauri build    # đóng gói installer (msi/nsis, dmg, AppImage/deb)

npm test               # test frontend (Vitest)
npm run lint           # ESLint, gồm rule cấm text cứng trong JSX
npm run i18n:check     # kiểm tra en/vi đủ và khớp key
cargo test --workspace # test Rust
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

CI ([.github/workflows/ci.yml](.github/workflows/ci.yml)) chạy toàn bộ kiểm tra trên cả 3 hệ điều hành và đóng gói installer khi push lên `main`.

## Cấu trúc

```
crates/texopt-core/   Thuật toán thuần Rust, không phụ thuộc Tauri (ops, atlas, rename, mesh, io, output, thumbs)
src-tauri/            Lệnh Tauri, hàng đợi job, cache session, protocol thumb://, tiến trình phụ xử lý 3D
src/                  React app
  app/                Thanh tab, TabHost (sleep/awake), Settings
  components/         ImageGrid, ParamForm, ToolLayout, CompareView, UI primitives
  stores/             zustand: tabs, session (undo/redo), jobs, presets, settings
  tabs/<tool>/        Mỗi công cụ một thư mục; đăng ký trong src/tabs/registry.ts
  locales/{en,vi}/    Chuỗi giao diện, mỗi namespace một file JSON
docs/                 Thiết kế (superpowers/specs) và kết quả spike 3D
```

## Đa ngôn ngữ

Code chỉ dùng **text ID** (`t('atlas:params.padding.label')`), không viết text trực tiếp; lint sẽ báo lỗi nếu có. Chuỗi hiển thị nằm trong `src/locales/en/*.json` và `src/locales/vi/*.json`; sửa nội dung ở đó rồi chạy `npm run i18n:check`. Backend Rust chỉ trả mã lỗi `{ code, params }`, giao diện dịch qua `errors.json`.

## Hạn chế đã biết

- Định dạng metadata Unity / Godot / Paper2D và file model xuất ra mới được kiểm tra bằng test tự động, chưa import thử vào engine thật.
- FBX dùng geometric pivot (kiểu 3ds Max) không ghi lại được an toàn; tool tự phát hiện và chuyển sang chế độ UV Remap Data.
- Ảnh đọc vào là RGBA 8-bit; không hỗ trợ GIF/TIFF/PSD, ảnh 16-bit hoặc HDR.
