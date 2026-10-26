<p align="center">
  <img src="assets/icon.svg" width="72" height="72" alt="Shiftwheel">
</p>

<h1 align="center">Shiftwheel</h1>

<p align="center">
  <b>Convert files from a radial wheel: drag a file, tap a key, drop it on a slice.</b><br>
  Built with <b>Tauri v2 + Rust</b> for Windows 10/11. Ultra-lightweight (1.29 MB installer, ~30 MB RAM).<br>
  Everything runs locally. No window, no account, no telemetry, no network.
</p>

---

| Key | Wheel | What's on it |
|---|---|---|
| <kbd>Shift</kbd> | **Convert** | Image, PDF, document, spreadsheet, presentation, audio, and video formats |
| <kbd>Ctrl</kbd> | **Tools** | Compress, 50%, crop, trim borders, square 1:1, rotate, grayscale, mirror, mute, speed up |
| <kbd>Alt</kbd> | **Actions** | Copy path, copy file to clipboard (`CF_HDROP`), copy/move to folder, open in Terminal, reveal in Explorer |

Tap another key mid-drag to switch wheels. Drop on the center hole, release outside the wheel, or press <kbd>Esc</kbd> to cancel. Output files are saved next to the original (`report.pdf`, `photo (compressed).jpg`, `name (1).ext`) — originals are never modified or overwritten.

Jobs that take longer than 600 ms show a small progress card in the bottom-right corner. Clicking the completion notification highlights the new file in Explorer.

---

## Convert (<kbd>Shift</kbd>)

| Input | From | To | Backend |
|---|---|---|---|
| **Images** | `png` `jpg` `webp` `gif` `tiff` `avif` `bmp` `heic` `svg` | `JPG` `PNG` `WEBP` `AVIF` `TIFF` `GIF` `BMP` `PDF` `TXT` | Rust (`image` crate), Windows OCR (`bmp`/`heic` via `ffmpeg`) |
| **PDF** | `pdf` | `DOCX` `TXT` `JPG` `PNG` | LibreOffice, `pdftoppm`, Windows OCR |
| **Documents** | `doc` `docx` `odt` `rtf` `txt` `html` `md` | `PDF` `DOCX` `ODT` `RTF` `TXT` `HTML` | LibreOffice |
| **Spreadsheets** | `xls` `xlsx` `ods` `csv` | `PDF` `XLSX` `ODS` `CSV` | LibreOffice |
| **Presentations** | `ppt` `pptx` `odp` | `PDF` `PPTX` `ODP` | LibreOffice |
| **Audio** | `mp3` `m4a` `wav` `flac` `ogg` `opus` `aiff` `wma` `aac` | `MP3` `M4A` `WAV` `FLAC` `OGG` `OPUS` `AIFF` | `ffmpeg` |
| **Video** | `mp4` `mov` `mkv` `webm` `avi` `wmv` `m4v` | `MP4` `MOV` `MKV` `WEBM` `GIF` `MP3` | `ffmpeg` |

- **Image → JPG**: applies EXIF orientation, flattens alpha onto white, high-quality JPEG.
- **Image → TXT**: uses Windows' built-in offline OCR. Small images are upscaled 2× first so small type stays readable. Images with no text are rejected instead of writing an empty file.
- **PDF → JPG / PNG**: 150 DPI per page (`name.jpg` for 1 page; `name 1.jpg`, `name 2.jpg`, … for multiple pages).
- **PDF → TXT**: extracts text frames through FODT sorted by page position so columns and tables read left-to-right. Scanned PDFs with no text layer automatically fall back to 200 DPI rendering + Windows OCR.
- **Video**: `MP4`/`MOV`/`MKV` use H.264 + AAC (CRF 20). `WEBM` uses VP9 + Opus. `GIF` is 12 fps, max 480 px wide.

---

## Tools (<kbd>Ctrl</kbd>)

| Category | Available tools |
|---|---|
| **Images** | **Compress** (max 2560 px; skipped if not smaller) · **50%** scale · **Rotate** 90° · **Grayscale** · **Mirror** · **Crop** (interactive editor with `Free`, `1:1`, `4:3`, `3:2`, `16:9`, `9:16` presets) · **Trim edges** (cuts solid borders on screenshots/scans) · **Square** (`1:1` center crop) |
| **PDF** | **Merge** *(2+ PDFs)* · **Split** into single-page PDFs · **Rotate** all pages 90° |
| **Video** | **Compress** (max 1080p, CRF 28) · **720p** · **Mute** (stream copy, no re-encode) · **Rotate** 90° · **Extract Audio** (saves sound as `.mp3`) · **2× speed** |
| **Audio** | **Compress** (128 kbps MP3) · **Mono** · **Cut edges** (trim silence) · **2× speed** · **Fade** in/out |

---

## File Actions (<kbd>Alt</kbd>)

Works on files, folders, or mixed selections:

- **Copy path** — full path(s) to the clipboard as text, one per line.
- **Copy** — puts the files on the Windows clipboard (`CF_HDROP`), ready to paste with <kbd>Ctrl</kbd>+<kbd>V</kbd>.
- **Copy to / Move to** — pick a target folder; name collisions get `(1)`, `(2)`.
- **Terminal** — opens the folder in Windows Terminal (`wt.exe`), or `cmd.exe` if unavailable.
- **Explorer** — highlights the item in File Explorer.

---

## Helper Programs

Image conversions, image tools, and Windows actions need nothing extra. For office documents and video conversions, Shiftwheel looks for these tools in standard paths and `PATH`:

| Program | Needed for | Install |
|---|---|---|
| [FFmpeg](https://www.gyan.dev/ffmpeg/builds/) | Audio & video conversions, `BMP`/`HEIC` | `winget install Gyan.FFmpeg` |
| [LibreOffice](https://www.libreoffice.org/download/) | Office documents (`DOCX`, `XLSX`, `PPTX` to `PDF`) | `winget install TheDocumentFoundation.LibreOffice` |
| Windows OCR | `Image → TXT`, scanned `PDF → TXT` | Built into Windows 10/11 |

---

## Install & Build

### Download Prebuilt Installer
Download `Shiftwheel_1.0.0_x64-setup.exe` (**~1.29 MB**) from the [Releases](https://github.com/Hesyitena/shiftwheel/releases) page. Installs per-user into `%LOCALAPPDATA%\Programs\Shiftwheel` (no UAC / admin prompt needed) and runs in the system tray.

### Build from Source

Prerequisites:
- [Rust](https://rustup.rs/) (1.80+)
- [Node.js](https://nodejs.org/) (20+)

```bash
# Clone and install dependencies
git clone https://github.com/Hesyitena/shiftwheel.git -b feat/tauri-migration
cd shiftwheel
npm install

# Run in development mode
npm run dev

# Build the 1.29 MB NSIS installer
npm run dist
```

Output installer is placed at `src-tauri/target/release/bundle/nsis/Shiftwheel_1.0.0_x64-setup.exe` (and copied to `dist/`).

| Script | Description |
|---|---|
| `npm run dev` | Run Shiftwheel with Tauri dev mode |
| `npm run build` | Build optimized release binary (`src-tauri/target/release/shiftwheel.exe`) |
| `npm run dist` | Build complete NSIS setup installer (`1.29 MB`) |

---

## Resource Usage & Architecture

Tauri v2 brings massive resource efficiency compared to classic Electron builds:

| Metrik | Electron | Tauri v2 | Penghematan |
|---|---|---|---|
| **Installer Size** | **103.58 MB** | **1.29 MB** | **-98.7%** 🔥 |
| **Installed Binary** | **~280 MB** | **3.43 MB** | **-98.8%** |
| **Idle Memory (RAM)** | **~85 MB** | **~28 MB** | **-67%** |

- **Native Windows Hooks**: Low-level `WH_MOUSE_LL` and `WH_KEYBOARD_LL` Windows hooks run in a lightweight background OS thread without Node.js or native addon overhead.
- **Hardware-Accelerated WebView2**: System-provided webview with CPU-efficient rendering.
- **Synthesized WebAudio**: Custom UI sound effects synthesized dynamically via WebAudio oscillators (closes audio context after 1.5s idle to save CPU).
- **Non-destructive drop**: Shiftwheel intercepts drops with `copy` effect, ensuring Windows Explorer never deletes or moves source files unintentionally.
