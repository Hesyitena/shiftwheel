# CLAUDE.md

Shiftwheel: Windows-only tray app (Tauri v2 + Rust). While dragging files, tap Shift / Ctrl / Alt to open a radial wheel; dropping on a slice converts the file, runs a tool, or performs a file action.

## Commands

```bash
npm install          # installs @tauri-apps/cli only
npm run dev          # tauri dev
npm run build        # release binary
npm run dist         # NSIS installer
npm test             # all tests (Rust + UI)
npm run test:rust    # cargo test --workspace (unit + end-to-end job tests)
npm run test:ui      # overlay/HUD/crop pages in Chromium (needs Playwright + Chromium)
```

The app targets Windows, but the crate also builds on Linux/macOS (Win32 code is behind `cfg(windows)`), so tests run anywhere. End-to-end job tests use the real FFmpeg / LibreOffice / Poppler when installed and skip themselves otherwise. To exercise the Win32 code from Linux: `cargo test --no-run --target x86_64-pc-windows-gnu` (needs mingw-w64) and run the test exe under Wine.

## Layout

- `src-tauri/src/lib.rs` – app setup, commands (`commands()`), overlay drag-drop handling, hook event handling, `TauriUi` (the job UI backed by real windows).
- `src-tauri/src/hook.rs` – global `WH_MOUSE_LL` / `WH_KEYBOARD_LL` hooks. Logic is the pure `Machine`; the hook procedures only feed it and send events through a channel.
- `src-tauri/src/wheel.rs` – which slices a selection gets (`items_for`) and hit-testing (`resolve`). The overlay draws exactly this list.
- `src-tauri/src/jobs.rs` – job queue (one worker thread), `run_job`, progress/result messages, the `Ui` trait.
- `src-tauri/src/converter.rs` – conversions and tools. `imaging.rs` (ffmpeg fallback around `imgcore`), `pdfops.rs` (lopdf merge/split/compress), `fodt.rs` (PDF text via LibreOffice), `external.rs` (finding and running ffmpeg/soffice/pdftoppm without console windows).
- `src-tauri/imgcore/` – workspace crate with the CPU-heavy image work (decode with EXIF, encode to memory, image PDFs, trim). Built with `opt-level = 3` while the app is built for size; keep generic `image` calls (encoders, pixel loops) in here, or they get compiled at the app's size-optimized level.
- `src-tauri/src/actions.rs` – clipboard, copy/move to folder (recursive), native folder picker, terminal, reveal in Explorer, `write_unique`.
- `src-tauri/src/window_manager.rs` – overlay/HUD placement in physical pixels using the target monitor's DPI and work area.
- `src-tauri/src/config.rs` – wheel contents (`convert:<fmt>`, `tool:<cat>.<id>`, `action:<id>`).
- `src-tauri/src/e2e.rs` – end-to-end tests: drop position → slice → job with real helper programs → output checked; IPC contract tests.
- `ui/` – static pages (no bundler): `overlay.html`, `hud.html`, `crop.html`, `bridge.js` (`window.shiftwheel`).
- `tests/ui-e2e.mjs` – UI tests with a recording fake of the Tauri bridge.

## Conventions and gotchas

- Wheel geometry (center 180, inner radius 80, outer 146, hit radius 166) lives in `wheel.rs` and `overlay.html::segmentAt`. The UI test checks them point by point against vectors exported from Rust.
- Every id in `config.rs` needs an implementation (`jobs::known_tool` / `known_action`); a test enforces it.
- Outputs go next to the source through `actions::write_unique` (claims the name with create-new, so parallel jobs can't collide). Never overwrite the source or an existing file (LibreOffice writes into a temp folder first for this reason).
- Batches of pure image work run on up to 4 threads (`jobs::per_file` with `parallel`); FFmpeg/LibreOffice jobs stay sequential.
- Release size matters: don't add `opt-level = 3` for the app crate itself (it pulls Tauri's generics along and nearly doubles the exe).
- The hook procedures run inside Windows' input pipeline: keep them fast, never call into Tauri or block there.
- Events and commands used by `bridge.js` must exist in Rust; `ipc_names_match_between_bridge_and_backend` checks this.
- Tray check items are toggled by the menu itself before the event arrives: read `is_checked()`, don't flip it again.
