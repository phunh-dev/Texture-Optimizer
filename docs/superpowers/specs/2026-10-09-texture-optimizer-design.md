# Texture Optimizer — Kế hoạch triển khai

## Context
Repo `Texture-Optimizer` hiện chỉ có `README.md` + `Requirement.md` (greenfield). Mục tiêu: một desktop app (Windows/macOS/Linux) gồm bộ công cụ tối ưu texture cho Unity/Unreal/Godot, hoạt động theo tab, UI đẹp, mỗi tính năng có đầy đủ tham số tùy chỉnh. Bonus: pack texture của nhiều model 3D (FBX/OBJ/DAE) thành atlas POT và remap UV để model vẫn dùng được.

**Quyết định đã chốt với user**
- Tech stack: **Tauri 2 + React + TypeScript (frontend) + Rust (xử lý ảnh/3D)**
- 3D formats: **.fbx, .obj, .dae**
- Atlas exporters: **Generic JSON (TexturePacker-style), Unity, Godot, Unreal (Paper2D)** — user chọn **1 exporter** mỗi lần xuất, mỗi exporter có options riêng.

## Tech stack chi tiết
| Lớp | Thư viện |
|---|---|
| Shell | Tauri 2 (`tauri-plugin-dialog`, `-fs`, `-store`, `-log`), `tauri-specta` sinh type TS cho commands |
| Frontend | React 19 + Vite + TS, Tailwind v4, shadcn/ui (Radix), lucide-react, zustand + `zundo` (undo/redo), `@tanstack/react-virtual` (grid ảo hóa), zod (schema tham số) |
| i18n | `i18next` + `react-i18next`, `eslint-plugin-i18next` (cấm text cứng trong JSX) |
| Ảnh (Rust) | `image` 0.25, `fast_image_resize`, `rayon` (song song), `oxipng` (tối ưu PNG – optional), `blake3` (hash dedupe/cache) |
| Packing | Tự viết MaxRects (BSSF/BLSF/BAF/BL/CP) + Skyline trong Rust (cần kiểm soát incremental) |
| 3D | `russimp` / `russimp-sys` (Assimp, static link) đọc FBX/OBJ/DAE; ghi qua `aiExportScene` (FFI) — **cần spike xác minh FBX export** |
| Test | `cargo test` (+ `proptest` cho packer), `vitest` + Testing Library |
| CI | GitHub Actions matrix (windows/macos/ubuntu) + `tauri-apps/tauri-action` |

## Kiến trúc
```
Texture-Optimizer/
├─ Cargo.toml                 # workspace
├─ crates/texopt-core/        # Rust thuần, KHÔNG phụ thuộc Tauri → test độc lập
│  └─ src/{io, ops/{resolution,trim,pot_pad,resize,bg_remove}, atlas/{packer,incremental,exporters/{generic,unity,godot,unreal}}, rename, mesh/{import,uv_remap,export}, thumbs}
├─ src-tauri/                 # commands mỏng gọi core, job queue, session/tab state, thumb:// protocol
├─ src/                       # React app
│  ├─ app/ (TabBar, TabHost, theme)
│  ├─ components/ImageGrid/ (virtual grid, size toggle S/M/L, ô "+", drop zone)
│  ├─ components/ParamForm/ (render control tự động từ zod schema)
│  ├─ tabs/<feature>/ (schema.ts + Panel.tsx + Preview.tsx) × 8
│  ├─ stores/ (zustand: tabs, sessions + history, presets)
│  ├─ i18n/ (khởi tạo i18next, hook useT, kiểu key sinh tự động)
│  └─ locales/
│     ├─ en/{common,tabs,errors,resize,resolution,trim,potpad,atlas,rename,bgremove,mesh}.json
│     └─ vi/{...cùng bộ file...}.json
└─ docs/superpowers/specs/2026-10-09-texture-optimizer-design.md
```

**Luồng dữ liệu**: Frontend gửi `{tabId, fileIds, params}` → command Rust đưa vào job queue (tokio + rayon) → emit event `progress`/`done` theo tabId → frontend cập nhật. Ảnh **không** đi qua IPC dạng base64: thumbnail sinh ở Rust, cache đĩa (`appCacheDir/thumbs/<blake3(path+mtime+size)>.webp`), phục vụ qua custom protocol `thumb://`.

**Tab sleep / awake**
- Chỉ tab active được mount (`TabHost` render đúng 1 tab). Tab ẩn chỉ giữ state nhẹ trong zustand: danh sách path, params, kết quả (path).
- Khi sleep: Rust drop buffer ảnh đã decode của session đó (LRU per-session), frontend bỏ `<img>` → browser tự giải phóng.
- Tab đang chạy job → trạng thái "busy", job vẫn chạy nền, chỉ ngủ phần UI; badge tiến độ hiển thị trên tab bar.
- Awake: re-mount, thumbnail lấy lại từ disk cache (nhanh).

## Thành phần UI chung (dùng cho mọi tab)
- **Import**: kéo-thả (Tauri `onDragDropEvent` lấy path thật), nút chọn file(s), chọn folder (tùy chọn recursive + filter đuôi file: png/jpg/tga/bmp/webp/psd-flatten?).
- **ImageGrid**: grid ảo hóa, cuộn dọc, 3 kích thước Big/Medium/Small, ô **"+"** cuối grid, drop thẳng vào grid; chọn nhiều ảnh (click/shift/ctrl); **badge "×" ở góc trên-phải mỗi cell** (hiện khi hover, luôn hiện ở chế độ Small) để xóa nhanh; phím Delete xóa các ảnh đang chọn; "Clear all"; badge kích thước + cảnh báo (non-POT, không chia hết cho 4). Xóa chỉ là gỡ khỏi danh sách, **không xóa file trên đĩa**.
- **Panel tham số** bên phải: sinh tự động từ zod schema; lưu/đọc **preset** cho từng tab; nút "Reset về mặc định".

## Undo / Redo
- Mỗi tab có **1 lịch sử riêng** (zustand + `zundo` temporal middleware), track phần state `{ files (kèm thứ tự), params }` (không track selection/scroll). Undo áp dụng cho: thêm ảnh, xóa ảnh (badge ×, Delete, Clear all), sắp xếp lại, đổi tham số, áp preset, reset tham số.
- **Gộp thay đổi liên tục**: kéo slider / gõ số được gộp thành 1 bước (debounce ~400ms, commit khi thả chuột/blur) để undo không phải bấm hàng chục lần.
- Giới hạn 100 bước/tab; lịch sử **giữ nguyên khi tab sleep** (chỉ là path + params, rất nhẹ); đóng tab thì bỏ.
- UI: nút Undo/Redo trên toolbar của tab (có tooltip mô tả bước, vd "Undo: Remove 3 images"), phím tắt `Ctrl/Cmd+Z`, `Ctrl/Cmd+Shift+Z` / `Ctrl+Y`; toast sau khi xóa có nút "Undo".
- Thao tác ghi file thật (output, Renamer rename tại chỗ) **không** nằm trong undo state; Renamer có "Revert last rename" riêng dựa trên rename log.

## Localization (English / Tiếng Việt)
- `i18next` + `react-i18next`; mỗi ngôn ngữ một thư mục `src/locales/<lang>/`, tách theo namespace (common, tabs, errors, từng tính năng) để dễ hiệu đính.
- **Code chỉ dùng text ID**, vd `t('atlas:params.padding.label')`; zod schema tham số chỉ chứa `labelKey`/`descKey`, không chứa text. `eslint-plugin-i18next` (`no-literal-string`) chặn text cứng trong JSX.
- Rust **không trả text hiển thị**: lỗi/cảnh báo trả `{ code: "IMG_DECODE_FAILED", params: {...} }`, frontend dịch qua `errors:<code>` (có interpolation).
- Script `npm run i18n:check` (chạy trong CI): so key giữa `en` và `vi` → báo thiếu/thừa; dev mode log key thiếu, fallback hiển thị sang `en`.
- Type-safe key: khai báo `CustomTypeOptions` của i18next từ file `en` để TS báo lỗi khi gõ sai ID.
- Chọn ngôn ngữ trong Settings (mặc định theo locale OS), lưu bằng `tauri-plugin-store`, đổi ngay không cần restart.
- **Preview**: before/after với thanh trượt so sánh, zoom, nền checker.
- **Output chung**: thư mục đích / ghi đè tại chỗ / thêm hậu tố; định dạng (giữ nguyên/PNG/TGA/JPG/WebP), mức nén PNG, oxipng on/off; xử lý xung đột tên.
- Theme sáng/tối, phím tắt, toast lỗi, nút Cancel cho job.

## Tính năng & tham số
1. **Resolution Fixer** — target: Multiple of 4 / Multiple of N / POT; làm tròn: nearest/up/down; phương pháp: Resample / Pad canvas / Crop; anchor 9 vị trí (cho pad/crop); filter resample; giữ tỷ lệ; cho phép POT không vuông; max size; màu nền pad.
2. **Sprite/Texture Trimmer** — alpha threshold (0–255); margin giữ lại; trim từng cạnh (bật/tắt riêng); snap kích thước sau trim (none/×4/POT); xuất JSON offset (để giữ pivot gốc).
3. **POT Padding** — target: next POT / square POT / kích thước cố định; anchor 9 vị trí; fill: trong suốt / màu / edge-extend; min/max size.
4. **Smart Atlas Generator** — thuật toán (MaxRects + heuristic, Skyline); max size; ép POT; ép vuông; padding; extrude (edge bleed); border; cho phép xoay; trim trước khi pack; dedupe ảnh trùng (hash); multi-page; thứ tự sort; premultiplied alpha; định dạng ảnh.
   - **Incremental**: file project `*.texatlas.json` (sprite name, hash, rect). Thêm ảnh → mode `Keep positions` (chèn vào free rect, giữ sprite cũ) hoặc `Repack optimal`; ảnh trùng tên → thay thế; luôn ghi đè output cũ.
   - **Exporter (chọn 1)** kèm options riêng:
     - Generic JSON: hash/array, đường dẫn ảnh tương đối, trimmed/rotated info.
     - Unity: sinh/ghi `.png.meta` với `spriteMode: Multiple`, PPU, pivot, filter mode, max size, compression; **giữ nguyên GUID + internalID của sprite cũ** khi incremental (không vỡ reference).
     - Godot: Godot 3 / Godot 4; `AtlasTexture .tres` cho từng sprite; base path `res://`; margin cho sprite bị trim.
     - Unreal: Paper2D JSON (format TexturePacker Paper2D), pivot.
5. **Pattern Renamer** — template token `{name} {index} {parent} {width} {height} {ext} {date}`; prefix/suffix; số bắt đầu, bước, zero-pad; đổi case (snake/kebab/camel/Pascal/lower/upper); find/replace (regex); **smart prefix/suffix** theo quy ước engine (vd Unreal `T_Name_N` khi phát hiện normal/albedo/roughness từ tên); sắp xếp trước khi đánh số; preview bảng cũ → mới, phát hiện xung đột; rename tại chỗ hoặc copy; **Undo** qua rename log.
6. **Background Remover** — mode: màu trắng / checker (tự dò 2 màu + kích thước ô) / màu tùy chọn (eyedropper); flood fill từ viền (liền kề) hoặc global; tolerance; metric RGB/Lab; feather mép; defringe (khử viền trắng/halo bằng un-blend với màu nền).
7. **Resize** — theo %, kích thước chính xác, fit width/height, cạnh dài nhất; giữ tỷ lệ; filter Nearest (pixel art)/Bilinear/CatmullRom/Mitchell/Lanczos3; resize trong linear space; xử lý premultiplied alpha; snap sau resize (none/×4/POT).
8. **Bonus — 3D Model Texture Packer**
   - Import `.fbx/.obj/.dae` (Assimp) → liệt kê mesh, material, texture từng kênh (albedo/normal/metallic/roughness/emission…).
   - Pack mỗi kênh thành atlas **cùng layout** (để normal/albedo khớp nhau), output **luôn POT**; padding + extrude cấu hình được (khuyến nghị 4–8px vì mipmap).
   - Remap UV: `uv' = offset + uv * scale` (inset nửa pixel). Phát hiện UV ngoài [0,1] (tiling) → option: cảnh báo & bỏ material đó khỏi atlas / clamp / bake lặp lại N lần.
   - Gộp material thành 1 material dùng chung (option) để giảm draw call.
   - Output: model đã remap (cùng định dạng hoặc chọn định dạng) + atlas từng kênh + report JSON.
   - **Fallback nếu không ghi được FBX tốt**: mode "UV Remap Data" — chỉ xuất atlas + JSON remap per-material, kèm script importer cho engine (Unity `AssetPostprocessor`) áp UV lúc import; không sửa file FBX.

## Lộ trình thực hiện (theo milestone)
- **M0 – Khởi tạo**: tạo `.gitignore` (bên dưới), copy plan này thành spec `docs/superpowers/specs/2026-10-09-texture-optimizer-design.md`, commit. Scaffold Tauri 2 + React/Vite/TS, Cargo workspace, Tailwind + shadcn, CI build 3 OS.
- **M1 – Spike 3D (sớm, để giảm rủi ro)**: dùng `russimp-sys` load FBX/OBJ/DAE, sửa UV, `aiExportScene` ra fbx/obj/dae; mở lại kết quả trong Unity/Blender. Ghi kết luận → chốt mode FBX (ghi trực tiếp hay fallback).
- **M2 – Khung app**: hạ tầng i18n (en/vi, lint rule, `i18n:check`) **làm đầu tiên** để mọi UI sau đó dùng text ID ngay từ đầu; TabBar/TabHost + sleep/awake; ImageGrid (virtual, S/M/L, "+", drag-drop, chọn folder, badge × xóa); history per-tab + Undo/Redo (toolbar, phím tắt, toast Undo); thumbnail cache + `thumb://`; job queue + progress/cancel; ParamForm từ zod (labelKey) + presets + reset; Output settings chung; Settings chọn ngôn ngữ.
- **M3 – Tính năng ảnh cơ bản**: Resize → Resolution Fixer → POT Padding → Trimmer (dùng chung core resample/pad/crop).
- **M4 – Background Remover + Pattern Renamer** (kèm preview & undo).
- **M5 – Smart Atlas**: packer + test, incremental project file, 4 exporter, preview atlas.
- **M6 – 3D Texture Packer** (dựa trên kết quả M1).
- **M7 – Hoàn thiện**: polish UI/UX, theme, phím tắt, đóng gói installer (msi/nsis, dmg, AppImage/deb), README.

Mỗi milestone triển khai theo TDD cho phần core Rust.

## Chiến lược thực thi song song (multi-agent)
**Wave 0 — Nền móng (tuần tự, tôi tự làm)**: kiểm tra toolchain (Rust, Node, CMake cho Assimp, WebView2/VS Build Tools); tạo `.gitignore`, spec doc; scaffold Tauri 2 + React/Vite/TS + Tailwind/shadcn; Cargo workspace với `texopt-core` (khung module rỗng + **các interface chung**: `ImageBuf`, `OpError {code, params}`, trait `Op { fn apply(&self, img) }`, struct params serde cho từng op); hạ tầng i18n + 2 file locale gốc; commit. Đây là "hợp đồng" để các agent sau không đụng nhau.

**Wave 1 — Song song (mỗi agent 1 git worktree riêng, chỉ sửa vùng file được giao)**
| Agent | Phạm vi file | Việc |
|---|---|---|
| A – Core image ops | `crates/texopt-core/src/ops/{resize,resolution,pot_pad,trim}` | 4 op + test golden/property |
| B – BG remover + Renamer core | `ops/bg_remove`, `rename/` | thuật toán + test |
| C – Atlas | `atlas/**` | MaxRects/Skyline, incremental project file, 4 exporter + test |
| D – Frontend shell | `src/app`, `src/components`, `src/stores`, `src/i18n` | TabBar/TabHost sleep, ImageGrid (S/M/L, +, badge ×, drag-drop), history/undo-redo, ParamForm, Settings ngôn ngữ |
| E – Tauri bridge | `src-tauri/**` | commands, job queue + progress/cancel, `thumb://` + disk cache, drag-drop/dialog/folder import, tauri-specta |
| F – 3D spike | `crates/texopt-core/src/mesh/**`, `spikes/` | russimp load/sửa UV/export FBX-OBJ-DAE → báo cáo kết luận |

Locale: mỗi agent chỉ thêm key vào namespace file của mình (`locales/{en,vi}/<feature>.json`) → không xung đột.

**Wave 2 — Song song (sau khi merge Wave 1)**: các agent làm UI từng tab tính năng (`src/tabs/<feature>/` schema + Panel + Preview) nối với command: (G) Resize/Resolution/POT/Trim, (H) BG Remover/Renamer, (I) Atlas, (J) 3D Packer theo kết quả spike.

**Wave 3 — Tích hợp (tôi làm)**: merge, chạy `cargo test`, `npm run test`, `npm run lint`, `npm run i18n:check`, `npm run tauri build`; review code; sửa lỗi tích hợp; polish UI.

Sau mỗi wave: merge worktree về `main` theo thứ tự, chạy full test trước khi mở wave tiếp theo.

## `.gitignore` (tạo ở M0)
```gitignore
# Node / frontend
node_modules/
dist/
dist-ssr/
*.local
.vite/
coverage/
npm-debug.log*
yarn-debug.log*
yarn-error.log*
pnpm-debug.log*
.pnpm-store/
*.tsbuildinfo

# Rust / Cargo
target/
**/*.rs.bk
*.pdb

# Tauri
src-tauri/target/
src-tauri/gen/schemas/
src-tauri/WixTools/

# Env / secrets
.env
.env.*
!.env.example
*.pem
*.key

# Test & sample output
/test-output/
/samples/output/
*.texatlas.cache

# Logs
logs/
*.log

# IDE / editor
.vscode/*
!.vscode/extensions.json
!.vscode/settings.json
.idea/
*.swp
*.swo
*.sublime-*

# OS
.DS_Store
Thumbs.db
ehthumbs.db
Desktop.ini
$RECYCLE.BIN/
*~
.directory
```
(Giữ `Cargo.lock` vì đây là app, không phải library.)

## Test cases bắt buộc theo tính năng
**Quy tắc (Definition of Done cho mọi agent)**: viết test trước (TDD), chạy → sửa → chạy lại **cho đến khi pass 100%**; không `#[ignore]`/`skip`, không nới assertion để cho qua. Agent chỉ được báo "xong" khi dán output test xanh. Tôi chạy lại toàn bộ suite sau mỗi lần merge; fail → giao lại agent sửa đến khi xanh. Ảnh test sinh bằng code (fixture generator) để không phụ thuộc file nhị phân.

- **Resize**: %, exact, fit W/H, longest side; giữ/không giữ tỷ lệ; mỗi filter cho đúng kích thước; Nearest giữ nguyên pixel art (không màu mới); upscale/downscale 1px, ảnh 1×1; premultiplied alpha không sinh viền đen; snap ×4/POT sau resize.
- **Resolution Fixer**: ×4 / ×N / POT với round nearest/up/down (vd 1023→1024, 1025→1024/2048, 3→4); mode Resample/Pad/Crop × 9 anchor đặt nội dung đúng chỗ; ảnh đã hợp lệ → không đổi (byte-identical); max size clamp; POT không vuông.
- **POT Padding**: next POT, square POT, fixed size; 9 anchor; fill trong suốt/màu/edge-extend kiểm pixel viền; ảnh đã POT → giữ nguyên; ảnh lớn hơn max → lỗi `code` đúng.
- **Trimmer**: ảnh trong suốt hoàn toàn (trả 1×1 hoặc lỗi theo option); không có pixel trong suốt → giữ nguyên; threshold biên (alpha = threshold); margin; trim từng cạnh; offset JSON khớp để khôi phục vị trí gốc (round-trip).
- **BG Remover**: nền trắng thuần, trắng có nhiễu (tolerance), checker 8/16/32px tự dò, màu tùy chọn; flood-fill không ăn vào vùng trắng bên trong vật thể (so với global); feather; defringe giảm halo (đo độ sáng viền).
- **Renamer**: từng token; prefix/suffix; start/step/zero-pad; mọi case transform; regex find/replace (cả regex lỗi → error code); smart prefix theo engine; sort trước khi đánh số; phát hiện trùng tên (trong batch và với file có sẵn); rename tại chỗ + revert từ log trả lại đúng tên; ký tự không hợp lệ trên Windows/macOS/Linux.
- **Atlas packer**: (proptest) không chồng lấn, nằm trong bounds, tôn trọng padding/extrude/border; POT/vuông khi bật; rotation đúng; dedupe; multi-page khi vượt max; sprite lớn hơn max → lỗi rõ ràng; deterministic (cùng input → cùng output).
- **Atlas incremental**: thêm ảnh ở mode Keep positions → rect cũ không đổi; thay ảnh cùng tên; xóa sprite; Repack optimal; project file round-trip.
- **Atlas exporters**: snapshot test cho Generic JSON (hash/array), Unity `.meta` (giữ GUID + internalID khi incremental), Godot 3/4 `.tres`, Unreal Paper2D JSON; parse lại output để kiểm hợp lệ.
- **3D Packer**: OBJ/DAE/FBX fixture nhỏ (2–3 model, mỗi model 1 texture) → atlas POT; UV sau remap trỏ đúng vùng (sample màu tại UV mới = màu texture gốc); UV ngoài [0,1] → cảnh báo/clamp/bake theo option; đa kênh (albedo+normal) cùng layout; export load lại được bằng Assimp.
- **Tauri bridge**: job queue chạy/cancel/progress; thumbnail cache hit/miss khi file đổi mtime; import folder recursive + filter đuôi; lỗi trả `{code, params}`.
- **Frontend**: tab sleep/awake (unmount + giữ state); grid S/M/L, ô "+", badge × xóa đúng ảnh; undo/redo (xóa → undo → redo, gộp slider, giới hạn 100 bước, còn nguyên sau sleep); ParamForm render từ schema + validate; preset save/load/reset; đổi ngôn ngữ runtime; `i18n:check` + lint no-literal-string.

## Verification
- **Core Rust**: `cargo test -p texopt-core` — golden-image test cho resize/trim/pad/bg-remove; proptest cho packer (không chồng lấn, nằm trong bounds, POT khi bật); test incremental (sprite cũ giữ nguyên vị trí, Unity meta giữ GUID/internalID); test renamer (template, xung đột, undo).
- **Frontend**: `npm run test` (vitest) cho ParamForm, store tab sleep/awake, grid; history: xóa → undo → redo khôi phục đúng danh sách/thứ tự, slider kéo liên tục chỉ tạo 1 bước, lịch sử còn nguyên sau sleep/awake.
- **i18n**: `npm run i18n:check` pass (en/vi đủ key), `npm run lint` không có text cứng; chuyển EN ↔ VI ở runtime, rà toàn bộ màn hình không còn key thô/text sót.
- **E2E thủ công**: `npm run tauri dev` → mở nhiều tab, kiểm tra RAM khi chuyển tab (Task Manager), import 500+ ảnh để test cuộn mượt, chạy từng tính năng.
- **Engine**: import atlas + metadata vào Unity, Godot 4, Unreal Paper2D; import model đã remap vào Unity/Blender để xác nhận UV đúng.
- **Cross-platform**: CI build pass trên Windows/macOS/Linux, chạy thử installer.
