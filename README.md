<p align="center">
  <img src="assets/icon.svg" width="72" height="72" alt="Shiftwheel">
</p>

<h1 align="center">Shiftwheel</h1>

<p align="center">
  Drag a file in Explorer, tap a key, drop it on a slice.<br>
  Everything runs locally on Windows 10/11. No window, no account, no network.
</p>

---

| Key | Wheel | What's on it |
|---|---|---|
| <kbd>Shift</kbd> | **Convert** | Image, PDF, document, spreadsheet, presentation, audio, and video formats |
| <kbd>Ctrl</kbd> | **Tools** | Compress, 50%, crop, trim borders, square 1:1, rotate, grayscale, strip metadata, merge/split PDF, denoise, normalize, mute, speed up |
| <kbd>Alt</kbd> | **Actions** | Copy path, copy file to clipboard, copy/move to folder, open in Terminal, reveal in Explorer |

Tap another key mid-drag to switch wheels. Drop on the center hole, release outside the wheel, or press <kbd>Esc</kbd> to cancel. Output files are saved next to the original (`report.pdf`, `photo (compressed).jpg`, `name (1).ext`) — originals are never modified or overwritten.

Jobs that take longer than 600 ms show a small progress card in the bottom-right corner. Clicking the completion notification highlights the new file in Explorer.

---

## Convert (<kbd>Shift</kbd>)

| Input | From | To | Backend |
|---|---|---|---|
| Images | `png` `jpg` `webp` `gif` `tiff` `avif` `bmp` `heic` `svg` | `JPG` `PNG` `WEBP` `AVIF` `TIFF` `GIF` `BMP` `PDF` `TXT` | `sharp`, `pdf-lib`, Windows OCR (`bmp`/`heic` via `ffmpeg`) |
| PDF | `pdf` | `DOCX` `TXT` `JPG` `PNG` | LibreOffice, `pdftoppm`, Windows OCR |
| Documents | `doc` `docx` `odt` `rtf` `txt` `html` `md` | `PDF` `DOCX` `ODT` `RTF` `TXT` `HTML` | LibreOffice |
| Spreadsheets | `xls` `xlsx` `ods` `csv` | `PDF` `XLSX` `ODS` `CSV` | LibreOffice |
| Presentations | `ppt` `pptx` `odp` | `PDF` `PPTX` `ODP` | LibreOffice |
| Audio | `mp3` `m4a` `wav` `flac` `ogg` `opus` `aiff` `wma` `aac` | `MP3` `M4A` `WAV` `FLAC` `OGG` `OPUS` `AIFF` | `ffmpeg` |
| Video | `mp4` `mov` `mkv` `webm` `avi` `wmv` `m4v` | `MP4` `MOV` `MKV` `WEBM` `GIF` `MP3` | `ffmpeg` |

- **Image → JPG**: applies EXIF orientation, flattens alpha onto white, quality 92. Animated `GIF ↔ WEBP` stays animated.
- **Image → TXT**: uses Windows' built-in offline OCR. Small images are upscaled 2× first so small type stays readable. Images with no text are rejected instead of writing an empty file.
- **Image → PDF**: single page, 595 pt wide (A4), height matched to the image aspect ratio.
- **PDF → JPG / PNG**: 150 DPI per page (`name.jpg` for 1 page; `name 1.jpg`, `name 2.jpg`, … for multiple pages).
- **PDF → TXT**: extracts text frames through FODT sorted by page position so columns and tables read left-to-right. Scanned PDFs with no text layer automatically fall back to 200 DPI rendering + Windows OCR.
- **Video**: `MP4`/`MOV`/`MKV` use H.264 + AAC (CRF 20). `WEBM` uses VP9 + Opus. `GIF` is 12 fps, max 480 px wide.

---

## Tools (<kbd>Ctrl</kbd>)

| Category | Available tools |
|---|---|
| **Images** | **Compress** (max 2560 px, mozjpeg/palette/WebP/AVIF; skipped if not smaller) · **50%** scale · **Rotate** 90° · **Grayscale** · **Metadata** (strip EXIF/GPS) · **Mirror** · **Crop** (interactive editor with `Free`, `1:1`, `4:3`, `3:2`, `16:9`, `9:16` presets; applies the same crop across multiple images) · **Trim edges** (cuts solid borders on screenshots/scans) · **Square** (`1:1` attention crop) · **Merge PDF** *(2+ images)* |
| **PDF** | **Merge** *(2+ PDFs)* · **Split** into single-page PDFs · **Rotate** all pages 90° · **Metadata** (strip author/title/XMP) · **Page numbers** (`1 / N` footer) · **Reverse** page order |
| **Video** | **Compress** (max 1080p, CRF 28) · **720p** · **Mute** (stream copy, no re-encode) · **Rotate** 90° · **Thumbnail** (midpoint `.jpg`) · **2× speed** |
| **Audio** | **Compress** (128 kbps MP3) · **Normalize** (`-16 LUFS` loudnorm) · **Mono** · **Denoise** (`afftdn`) · **Trim silence** (start & end) · **1.5× speed** (pitch kept) · **Fade** in/out |
| **Office** | **Merge PDF** *(2+ files into `Merged.pdf`)* · **Pages / Slides** (each page or slide as a `.png`) |

---

## File Actions (<kbd>Alt</kbd>)

Works on files, folders, or mixed selections:

- **Copy path** — full path(s) to the clipboard as text, one per line.
- **Copy** — puts the files on the Windows clipboard (`CF_HDROP`), ready to paste with <kbd>Ctrl</kbd>+<kbd>V</kbd>.
- **Copy to / Move to** — pick a target folder; name collisions get `(1)`, `(2)`.
- **Terminal** — opens the folder in Windows Terminal (`wt.exe`), or `cmd.exe` if unavailable.
- **Explorer** — highlights the item in File Explorer.

You can also convert without dragging: right-click a file in Explorer → **Show more options** → **Convert with Shiftwheel** → **PDF / DOCX / TXT / JPG**.

---

## Helper Programs

Image conversions, image tools, and PDF tools need nothing extra. For documents, audio/video, and PDF rendering, Shiftwheel looks for these in standard install paths, `winget` packages, and `PATH`:

| Program | Needed for | Install |
|---|---|---|
| [LibreOffice](https://www.libreoffice.org/download/) | Office documents, `PDF → DOCX/TXT` | `winget install TheDocumentFoundation.LibreOffice` |
| [FFmpeg](https://www.gyan.dev/ffmpeg/builds/) | Audio, video, `BMP`/`HEIC` images | `winget install Gyan.FFmpeg` |
| [Poppler](https://github.com/oschwartz10612/poppler-windows/releases) | `PDF → JPG/PNG`, scanned PDF OCR, Office `Pages/Slides` | `winget install oschwartz10612.Poppler` |
| Windows OCR | `Image → TXT`, scanned `PDF → TXT` | Built into Windows 10/11 (any installed OCR language pack) |

If one is missing when you drop a file, the notification gives you the `winget` command above.

---

## Install & Build

Requires Node.js 20+ to build from source:

```bash
npm install
npm run dist
```

Run `dist/Shiftwheel Setup <version>.exe` — installs per-user into `%LOCALAPPDATA%\Programs\Shiftwheel` (no admin prompt), registers the Explorer context menu, and starts in the system tray.

| Script | Description |
|---|---|
| `npm run dev` | Run from source and auto-restart on file save (closes any installed tray instance first) |
| `npm run install-local` | Build the installer into `dist/install-<time>` and silently replace the running tray app |
| `npm test` | Syntax check (`node --check`), CSP SHA-256 hash check, hotkey tests, and converter/tool tests |
| `node test/check.js --fix` | Rewrite CSP hashes in `overlay.html`, `hud.html`, and `crop.html` after editing inline styles/scripts |

---

## Resource Usage & Internals

Idle footprint is ~75 MB private RAM across 3 processes and ~0.4% of one CPU core:

- **CPU rendering only** (`app.disableHardwareAcceleration()`), trimming Chromium's GPU process from ~80 MB to ~15 MB.
- **In-process network service** (`NetworkServiceInProcess`) since the app never uses the network.
- **Lazy loading**: `sharp` (`libvips`) and `pdf-lib` load only when the first job runs.
- **Synthesized WebAudio**: UI sounds are generated on the fly with oscillators; the `AudioContext` closes 1.5 s after the last sound so it doesn't burn idle CPU.
- **OLE file-drag probe**: When a drag + key tap is detected, the overlay probes at 1% alpha and only reveals the wheel if Windows OLE confirms files are being dragged (or the cursor is held stationary on a drag), avoiding false triggers during text selection.
- **Non-destructive drop**: Holding <kbd>Shift</kbd> normally tells Explorer to move a file; Shiftwheel forces `dropEffect = 'copy'` so dropping on the wheel never deletes the source file.
