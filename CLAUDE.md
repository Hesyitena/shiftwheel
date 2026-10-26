# CLAUDE.md

Shiftwheel: Windows-only tray app (Tauri v2 + Rust). While dragging files, tap Shift / Ctrl / Alt to open a radial wheel; dropping on a slice converts the file, runs a tool, or performs a file action.

## Commands

```bash
npm install          # installs @tauri-apps/cli only
npm run dev          # tauri dev
npm run build        # release binary
npm run dist         # NSIS installer
cd src-tauri && cargo test   # converter unit tests (image conversions)
```

The crate depends on `windows-sys` and Win32 APIs, so it only builds for a Windows target. On Linux/macOS use `cargo check --target x86_64-pc-windows-msvc` (or `-gnu`) at most.

## Layout

- `src-tauri/src/lib.rs` – app setup, overlay `DragDrop` handler, `resolve_segment` (hit-testing in Rust), `execute_action` (runs the job on a worker thread and drives the HUD).
- `src-tauri/src/hook.rs` – global `WH_MOUSE_LL` / `WH_KEYBOARD_LL` hooks on their own thread. Detects "mouse down + drag + modifier tap" and emits `HookEvent`s.
- `src-tauri/src/window_manager.rs` – positions/shows the `overlay` and `hud` windows.
- `src-tauri/src/config.rs` – wheel contents (`convert:<fmt>`, `tool:<cat>.<id>`, `action:<id>`), sent to the overlay as `overlay:config`.
- `src-tauri/src/converter.rs` – conversions (`image` crate, ffmpeg, LibreOffice `soffice`) and `run_tool`.
- `src-tauri/src/actions.rs` – clipboard (text, `CF_HDROP`), copy/move to folder, terminal, reveal in Explorer.
- `src-tauri/src/settings.rs` – `%APPDATA%\Shiftwheel\settings.json` + autostart registry key.
- `src-tauri/src/tray.rs` – tray menu.
- `ui/` – static frontend (no bundler): `overlay.html` (wheel), `hud.html` (progress card), `crop.html` (crop editor, not wired up yet), `bridge.js` (Tauri event/invoke wrapper as `window.shiftwheel`).

## Conventions and gotchas

- The wheel geometry (center 180, inner radius 80, outer 146, hit radius 166) is duplicated in `lib.rs::resolve_segment` and `overlay.html::segmentAt`. Keep them in sync.
- Item ids in `config.rs` must have a matching branch in `converter::run_tool` / `execute_action`, otherwise the slice shows up but fails at runtime.
- Output files go next to the source and use `actions::unique_path`. Never overwrite the source or an existing file.
- Code in the hook procedures runs inside Windows' low-level hook callback. Keep it fast and never block on the Tauri main thread there.
