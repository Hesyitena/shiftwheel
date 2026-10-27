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

Slices that need a helper program you have not installed yet are dimmed and say which one (for example AVIF → `ffmpeg`). On the first start a small card explains the three keys and offers *Start with Windows*.

Every job shows a small progress card in the bottom-right corner (above the taskbar). When it finishes, the card says what was saved — or why it failed — and clicking it highlights the new file in Explorer. Jobs run one after another, so dropping more files while one is running simply queues them; within a job, batches of images are processed on several cores.

---

## Convert (<kbd>Shift</kbd>)

| Input | From | To | Backend |
|---|---|---|---|
| **Images** | `png` `jpg` `webp` `gif` `tiff` `avif` `bmp` `heic` `svg` | `JPG` `PNG` `WEBP` `AVIF` `TIFF` `GIF` `BMP` `PDF` `TXT` | Rust (`image` crate); `heic`/`avif`/`svg` input and `AVIF` output via `ffmpeg`; `TXT` via Windows OCR |
| **PDF** | `pdf` | `DOCX` `TXT` `JPG` `PNG` | LibreOffice; Poppler (`pdftotext`, `pdftoppm`) when installed |
| **Documents** | `doc` `docx` `odt` `rtf` `txt` `html` `md` | `PDF` `DOCX` `ODT` `RTF` `TXT` `HTML` | LibreOffice |
| **Spreadsheets** | `xls` `xlsx` `ods` `csv` | `PDF` `XLSX` `ODS` `CSV` | LibreOffice |
| **Presentations** | `ppt` `pptx` `odp` | `PDF` `PPTX` `ODP` | LibreOffice |
| **Audio** | `mp3` `m4a` `wav` `flac` `ogg` `opus` `aiff` `wma` `aac` | `MP3` `M4A` `WAV` `FLAC` `OGG` `OPUS` `AIFF` | `ffmpeg` |
| **Video** | `mp4` `mov` `mkv` `webm` `avi` `wmv` `m4v` | `MP4` `MOV` `MKV` `WEBM` `GIF` `MP3` | `ffmpeg` |
| **Other text files** | `json` `log` `xml` `yaml`, source code, … | `PDF` `DOCX` `ODT` `TXT` | LibreOffice |

The wheel never offers the format a file already has (no `PNG → PNG`), and folders or unknown binary files get no conversions.

- **Images**: EXIF orientation is applied (phone photos come out upright). JPG flattens transparency onto white (quality 90).
- **Image → PDF**: one A4 page (turned to match the image), image centered; small images are not blown up.
- **Image → TXT**: uses Windows' built-in offline OCR. Small images are upscaled 2× first so small type stays readable. Images with no text are rejected instead of writing an empty file.
- **PDF → JPG / PNG**: 150 DPI per page with Poppler (`name.png` for 1 page; `name (page 1).png`, `name (page 2).png`, … for more). Without Poppler, LibreOffice renders the first page only.
- **PDF → TXT**: Poppler's `pdftotext` when installed; otherwise LibreOffice's PDF import, with the text frames sorted by page and position. Scanned PDFs without a text layer report that instead of writing an empty file.
- **Video**: `MP4`/`MOV`/`MKV` use H.264 + AAC (CRF 20). `WEBM` uses VP9 + Opus. `GIF` is 12 fps, max 480 px wide.

---

## Tools (<kbd>Ctrl</kbd>)

| Category | Available tools |
|---|---|
| **Images** | **Compress** (max 2560 px; skipped if not smaller) · **50%** size · **Fit 1080p** (only scales down) · **Square** (`1:1` center crop) · **Crop** (interactive editor with `Free`, `1:1`, `4:3`, `3:2`, `16:9`, `9:16` presets; one crop for all selected images) · **Rotate** 90° · **Flip** (mirror) · **Black & white** · **Trim border** (cuts plain margins on screenshots/scans) · **To PDF** *(2+ images, one page each)* |
| **PDF** | **Compress** (skipped if not smaller) · **Extract pages** (one PDF per page) · **Extract images** (pages as PNG, needs Poppler) · **Merge** *(2+ PDFs)* |
| **Audio** | **Cut edges** (trim silence) · **Fade** in/out · **2× speed** (same pitch) · **To mono** |
| **Video** | **Remove audio** (stream copy, no re-encode) · **Extract audio** (`.mp3`) · **2× speed** · **720p** (only scales down) · **Reverse** |

---

## File Actions (<kbd>Alt</kbd>)

Works on files, folders, or mixed selections:

- **Copy path** — full path(s) to the clipboard as text, one per line.
- **Copy** — puts the files on the Windows clipboard (`CF_HDROP`), ready to paste with <kbd>Ctrl</kbd>+<kbd>V</kbd>.
- **Copy to / Move to** — pick a target folder in the standard Windows folder dialog; name collisions get `(1)`, `(2)`.
- **Terminal** — opens the folder in Windows Terminal (`wt.exe`), or `cmd.exe` if unavailable.
- **Explorer** — highlights the item in File Explorer.

---

## Helper Programs

Image conversions, image tools, PDF merge/split/compress, and Windows actions need nothing extra. For the rest, Shiftwheel looks for these programs on `PATH` and in their usual install folders (`Program Files`, WinGet, Scoop, Chocolatey). When one is missing, the progress card says which one:

| Program | Needed for | Install |
|---|---|---|
| [FFmpeg](https://www.gyan.dev/ffmpeg/builds/) | Audio & video, `AVIF` output, `HEIC`/`AVIF`/`SVG` input | `winget install Gyan.FFmpeg` |
| [LibreOffice](https://www.libreoffice.org/download/) | Office documents (`DOCX`, `XLSX`, `PPTX` to `PDF`), `PDF → DOCX` | `winget install TheDocumentFoundation.LibreOffice` |
| [Poppler](https://github.com/oschwartz10612/poppler-windows) *(optional)* | Better `PDF → TXT`, all pages for `PDF → JPG/PNG` and **Extract images** | `winget install oschwartz10612.Poppler` |
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
| `npm run dist` | Build complete NSIS setup installer |
| `npm test` | Rust tests (incl. end-to-end jobs with real FFmpeg/LibreOffice/Poppler when installed) + UI tests in Chromium |

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
