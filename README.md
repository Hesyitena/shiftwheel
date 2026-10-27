<p align="center">
  <img src="assets/icon.svg" width="72" height="72" alt="Shiftwheel">
</p>

<h1 align="center">Shiftwheel</h1>

<p align="center">
  <b>Convert files from a radial wheel: drag a file, tap a key, drop it on a slice.</b><br>
  Built with <b>Tauri v2 + Rust</b> for Windows 10/11 (2.25 MB installer, ~28 MB RAM).<br>
  Runs entirely locally. No account, no telemetry, no network calls.
</p>

<p align="center">
  <a href="https://github.com/Hesyitena/shiftwheel/releases"><img src="https://img.shields.io/badge/Release-v1.1.0-e02424?style=flat-square" alt="Release"></a>
  <img src="https://img.shields.io/badge/Platform-Windows%2010%20%7C%2011-0078D6?style=flat-square&logo=windows&logoColor=white" alt="Platform">
  <img src="https://img.shields.io/badge/Framework-Tauri%20v2-24C8D8?style=flat-square&logo=tauri&logoColor=white" alt="Tauri v2">
  <img src="https://img.shields.io/badge/Backend-Rust-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Frontend-Vanilla%20JS%20%2B%20SVG-F7DF1E?style=flat-square&logo=javascript&logoColor=black" alt="Vanilla JS">
  <img src="https://img.shields.io/badge/Runtime-WebView2-008AD7?style=flat-square&logo=microsoft-edge&logoColor=white" alt="WebView2">
  <img src="https://img.shields.io/badge/Audio-WebAudio%20API-6C5CE7?style=flat-square" alt="WebAudio">
  <img src="https://img.shields.io/badge/License-MIT-44CC11?style=flat-square" alt="License">
</p>

---

| Key | Wheel | Contents |
|---|---|---|
| <kbd>Shift</kbd> | **Convert** | Image, PDF, document, spreadsheet, presentation, audio, and video formats |
| <kbd>Ctrl</kbd> | **Tools** | Compress, 50%, crop, trim borders, square 1:1, rotate, grayscale, mirror, mute, speed up |
| <kbd>Alt</kbd> | **Actions** | Copy path, copy file to clipboard (`CF_HDROP`), copy/move to folder, open in Terminal, reveal in Explorer |

The wheel only opens while files are actually being dragged (from File Explorer or the desktop), so holding a modifier during a rubber-band selection, a window drag, or a text selection does nothing. Tap another modifier key mid-drag to switch wheels. Double-tap a modifier without dragging to preview its wheel. Drop on the center hole, release outside the wheel, or press <kbd>Esc</kbd> to cancel. Output files are saved alongside the original (`photo (compressed).jpg`, `name (1).ext`) — originals are never overwritten.

Slices that need a helper program not yet installed are dimmed and display the required tool (e.g., AVIF → `ffmpeg`). The center ring displays the dragged file's thumbnail, name, and target slice in real time.

When a job runs, a progress card appears in the bottom-right corner above the taskbar. On completion, it shows the saved file size and buttons to open the file or its folder. The Windows shell is notified immediately via `SHChangeNotify`, so new files show up in File Explorer without pressing F5.

---

## Convert (<kbd>Shift</kbd>)

| Input | Formats | Output | Backend |
|---|---|---|---|
| **Images** | `png` `jpg` `webp` `gif` `tiff` `avif` `bmp` `heic` `svg` | `JPG` `PNG` `WEBP` `AVIF` `TIFF` `GIF` `BMP` `PDF` `TXT` | Rust (`image` crate + Rayon); `heic`/`avif`/`svg` input and `AVIF` output via `ffmpeg`; `TXT` via Windows OCR |
| **PDF** | `pdf` | `DOCX` `TXT` `JPG` `PNG` | LibreOffice; Poppler (`pdftotext`, `pdftoppm`) when installed |
| **Documents** | `doc` `docx` `odt` `rtf` `txt` `html` `md` | `PDF` `DOCX` `ODT` `RTF` `TXT` `HTML` | LibreOffice |
| **Spreadsheets** | `xls` `xlsx` `ods` `csv` | `PDF` `XLSX` `ODS` `CSV` | LibreOffice |
| **Presentations** | `ppt` `pptx` `odp` | `PDF` `PPTX` `ODP` | LibreOffice |
| **Audio** | `mp3` `m4a` `wav` `flac` `ogg` `opus` `aiff` `wma` `aac` | `MP3` `M4A` `WAV` `FLAC` `OGG` `OPUS` `AIFF` | `ffmpeg` |
| **Video** | `mp4` `mov` `mkv` `webm` `avi` `wmv` `m4v` | `MP4` `MOV` `MKV` `WEBM` `GIF` `MP3` | `ffmpeg` |
| **Plain Text & Code** | `json` `log` `xml` `yaml`, source code, ... | `PDF` `DOCX` `ODT` `TXT` | LibreOffice |

- Formats matching the input file are omitted from the wheel (e.g. no `PNG → PNG`).
- **Images**: EXIF orientation is preserved. JPG exports flatten transparency onto a white background (quality 90).
- **Image → PDF**: Centers the image on an A4 page matching the image aspect ratio without upscaling small assets.
- **Image → TXT**: Uses Windows built-in offline OCR API.
- **PDF → JPG / PNG**: Renders at 150 DPI per page via Poppler (`name.png` for single page; numbered suffix for multi-page). Falls back to first page rendering via LibreOffice if Poppler is absent.
- **PDF → TXT**: Uses Poppler's `pdftotext` when available, falling back to LibreOffice PDF text import.
- **Video**: `MP4`/`MOV`/`MKV` encode with H.264 + AAC (CRF 20). `WEBM` encodes with VP9 + Opus. `GIF` exports run at 12 fps, max 480 px width.

---

## Tools (<kbd>Ctrl</kbd>)

| Category | Tools |
|---|---|
| **Images** | **Compress** (max 2560 px; skipped if not smaller) · **50%** resize · **Fit 1080p** (downscale only) · **Square** (`1:1` center crop) · **Crop** (interactive editor with `Free`, `1:1`, `4:3`, `3:2`, `16:9`, `9:16` presets) · **Rotate** 90° · **Flip** (horizontal mirror) · **Grayscale** · **Trim border** (removes solid margins) · **To PDF** *(merges multiple selected images)* |
| **PDF** | **Compress** · **Extract pages** (individual single-page PDFs) · **Extract images** (PNG per page, requires Poppler) · **Merge** *(merges multiple selected PDFs)* |
| **Audio** | **Cut edges** (trim silence) · **Fade** in/out · **2× speed** (preserves pitch) · **To mono** |
| **Video** | **Remove audio** (stream copy, no re-encode) · **Extract audio** (`.mp3`) · **2× speed** · **720p** (downscale only) · **Reverse** |

---

## File Actions (<kbd>Alt</kbd>)

Applies to files, folders, or multi-item selections:

- **Copy path** — Copies full paths to the clipboard as text (one per line).
- **Copy** — Copies files to the clipboard as `CF_HDROP` for standard pasting.
- **Copy to / Move to** — Opens the native Windows folder picker to copy or move files.
- **Terminal** — Opens the containing folder in Windows Terminal (`wt.exe`), falling back to `cmd.exe`.
- **Explorer** — Selects and highlights the item in File Explorer.

---

## Helper Programs

Basic image conversions, image tools, PDF merge/split, and Windows actions run in native Rust without dependencies. For other formats, Shiftwheel detects helper binaries on `PATH` or standard installation directories:

| Program | Required For | Install via WinGet |
|---|---|---|
| [FFmpeg](https://www.gyan.dev/ffmpeg/builds/) | Audio/video conversions, `AVIF` output, `HEIC`/`AVIF`/`SVG` input | `winget install Gyan.FFmpeg` |
| [LibreOffice](https://www.libreoffice.org/download/) | Documents, spreadsheets, presentations (`DOCX`, `XLSX`, `PPTX` to `PDF`), `PDF → DOCX` | `winget install TheDocumentFoundation.LibreOffice` |
| [Poppler](https://github.com/oschwartz10612/poppler-windows) *(optional)* | Multi-page `PDF → PNG/JPG`, `PDF → TXT`, and image extraction | `winget install oschwartz10612.Poppler` |
| Windows OCR | `Image → TXT` | Built into Windows 10/11 |

---

## Tech Stack & Architecture

| Component | Technology | Role |
|---|---|---|
| **App Framework** | [Tauri v2](https://tauri.app/) | Native windowing, IPC, system tray, and packaging |
| **Backend Core** | [Rust](https://www.rust-lang.org/) | Core conversions, job scheduler, and memory-safe OS integration |
| **OS Hooks** | Win32 API (`windows-sys`) | Low-level `WH_MOUSE_LL` and `WH_KEYBOARD_LL` input monitoring |
| **Shell Integration** | Win32 Shell API | Native folder picker, `SHChangeNotify` instant Explorer refresh |
| **Image Engine** | [`image`](https://crates.io/crates/image) + [Rayon](https://crates.io/crates/rayon) | Multi-core parallel image decoding, encoding, and filters |
| **Frontend UI** | HTML5 / CSS3 / Inline SVG | Dot design radial wheel, crop canvas, and progress HUD |
| **Webview Runtime** | Microsoft Edge WebView2 | Evergreen system-provided web engine |
| **Sound Engine** | WebAudio API | Dynamic oscillator SFX generation without external audio files |
| **Packaging** | NSIS | Lightweight per-user standalone installer (~2.25 MB) |

---

## Resource Usage

| Metric | Electron (v0.x) | Tauri v2 + Rust (v1.x) | Difference |
|---|---|---|---|
| **Installer Size** | 103.58 MB | 2.25 MB | -97.8% |
| **Installed Size** | ~280 MB | 4.8 MB | -98.3% |
| **Idle RAM** | ~85 MB | ~28 MB | -67% |
| **Startup Time** | ~1.8s | < 300ms | 6× faster |

---

## Install & Build

### Download

Grab the installer or standalone binary from the [Releases](https://github.com/Hesyitena/shiftwheel/releases) page:
- `Shiftwheel_1.1.0_x64-setup.exe` (NSIS setup, installs per-user to `%LOCALAPPDATA%\Programs\Shiftwheel`, no admin rights required).
- `shiftwheel.exe` (portable executable).

### Build from Source

Prerequisites:
- [Rust](https://rustup.rs/) (1.80+)
- [Node.js](https://nodejs.org/) (20+)

```bash
git clone https://github.com/Hesyitena/shiftwheel.git
cd shiftwheel
npm install

# Run in development mode
npm run dev

# Build release binary
npm run build

# Build NSIS installer
npm run dist
```

The binary is written to `src-tauri/target/release/shiftwheel.exe` and the installer to `src-tauri/target/release/bundle/nsis/`.

---

## License

[MIT](LICENSE)
