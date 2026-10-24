# Shiftwheel

A radial drag-and-drop file converter and toolkit for Windows 11. Start dragging a file in Explorer, **tap** a key, and a wheel opens right around your cursor. It stays open until you drop the file:

| Tap while dragging | Wheel | What it does |
|---|---|---|
| **Shift** | **Convert** | Convert between image, PDF, document, spreadsheet, presentation, audio, and video formats |
| **Ctrl** | **Tools** | One-click utilities: compress, crop, resize, rotate, strip metadata, merge/split PDF, denoise audio, and more |
| **Alt** | **File actions** | Copy file path, copy files to clipboard, copy/move to folder, open in Terminal, or reveal in Explorer |

---

## Key Features

- **Cursor-Centered Radial Menu**: Opens right under your cursor while dragging one or more files. Tap **Shift**, **Ctrl**, or **Alt** mid-drag to switch between wheels on the fly.
- **Context-Aware Options**: Automatically detects the dragged file category (image, PDF, document, spreadsheet, presentation, audio, video) and displays only relevant formats and tools. Multi-file tools (like **Merge PDF**) appear automatically when dragging 2 or more files.
- **Non-Destructive by Default**: Original files are never modified or overwritten. Outputs are saved next to the source file (`report.pdf`, `photo (compressed).jpg`, or `name (1).ext` if a filename already exists).
- **Interactive Crop Editor**: Includes a dedicated, hardware-free crop window with aspect-ratio presets (**Free**, **1:1**, **4:3**, **3:2**, **16:9**, **9:16**), rule-of-thirds grid, 8-handle resizing, and batch cropping across multiple images at once.
- **Built-In Offline OCR**: Extracts text from images (`Image → TXT`) and scanned/photographed PDFs (`PDF → TXT`) using Windows' built-in OCR engine — completely offline.
- **Live Progress HUD & Clickable Notifications**: Jobs taking longer than 0.6 seconds display a compact progress card in the bottom-right corner (with real-time percentage for audio/video jobs). Clicking the completion notification reveals the new file in Explorer.
- **Zero-Asset Synthesized Sound Effects**: Crisp, subtle UI sounds (`open`, `close`, `tick`, `switch`, `select`, `cancel`, `done`, `error`) synthesized in real time via the WebAudio API — no audio files bundled. Can be toggled anytime from the system tray.
- **Explorer Context Menu Integration**: Right-click any file in Explorer → **Show more options** → **Convert with Shiftwheel** → **PDF / DOCX / TXT / JPG** even without dragging.
- **100% Offline & Lightweight**: No internet connection, no telemetry, and no uploads. Tuned to use ~75 MB of RAM while idle in the system tray.

---

## 1. Convert Wheel (`Shift`)

Tap **Shift** while dragging to convert files between formats:

| Category | Input Formats (`From`) | Target Formats (`To`) | Engine Used |
|---|---|---|---|
| **Images** | `png`, `jpg`, `webp`, `gif`, `tiff`, `avif`, `bmp`, `heic`, `svg` | **JPG**, **PNG**, **WEBP**, **AVIF**, **TIFF**, **GIF**, **BMP**, **PDF**, **TXT** | `sharp`, `pdf-lib`, Windows OCR (`bmp`/`heic` via `ffmpeg`) |
| **PDF** | `pdf` | **DOCX**, **TXT**, **JPG**, **PNG** | LibreOffice, `pdftoppm`, Windows OCR |
| **Documents** | `doc`, `docx`, `odt`, `rtf`, `txt`, `html` | **PDF**, **DOCX**, **ODT**, **RTF**, **TXT**, **HTML** | LibreOffice |
| **Spreadsheets** | `xls`, `xlsx`, `ods`, `csv` | **PDF**, **XLSX**, **ODS**, **CSV** | LibreOffice |
| **Presentations** | `ppt`, `pptx`, `odp` | **PDF**, **PPTX**, **ODP** | LibreOffice |
| **Audio** | `mp3`, `m4a`, `wav`, `flac`, `ogg`, `opus`, `aiff`, `wma`, `aac` | **MP3**, **M4A**, **WAV**, **FLAC**, **OGG**, **OPUS**, **AIFF** | `ffmpeg` |
| **Video** | `mp4`, `mov`, `mkv`, `webm`, `avi`, `wmv`, `m4v` | **MP4**, **MOV**, **MKV**, **WEBM**, **GIF**, **MP3** | `ffmpeg` |

### Conversion Details
- **Image → JPG**: Follows EXIF orientation automatically, flattens transparency onto white, and encodes at quality 92. Animated **GIF ↔ WEBP** conversions preserve animation frames.
- **Image → TXT (OCR)**: Reads text offline using Windows' built-in OCR. Small images are upscaled 2× first so fine print remains accurate. Images with no recognizable text are skipped rather than creating an empty `.txt` file.
- **Image → PDF**: Creates a clean single-page PDF (595 pt / A4 width) matching the image's aspect ratio.
- **PDF → JPG / PNG**: Renders every page at 150 DPI (`name.jpg` for single-page PDFs, or `name 1.jpg`, `name 2.jpg`, … for multi-page documents).
- **PDF → TXT**: Reads text frames via FODT and sorts them by page coordinates so multi-column and table layouts read naturally from left to right. If a PDF is scanned (contains images but no text layer), Shiftwheel automatically renders each page at 200 DPI and runs Windows OCR.
- **PDF → DOCX**: Converts via LibreOffice so text boxes remain editable in Word.
- **Video Conversions**: **MP4 / MOV / MKV** use H.264 + AAC (`CRF 20`); **WEBM** uses VP9 + Opus; **GIF** renders at 12 fps (max 480 px wide); **MP3** extracts the audio track.
- **Smart Skip**: Files that are already in the chosen target format are skipped automatically.

---

## 2. Tools Wheel (`Ctrl`)

Tap **Ctrl** while dragging to run one-click tools. Results are saved as new suffixed files (e.g. `photo (compressed).jpg`) so originals are never altered.

### Image Tools
| Tool | Description |
|---|---|
| **Compress** | Resizes to max 2560 px and optimizes encoding (`mozjpeg` / palette PNG / WebP / AVIF). Skipped automatically if the output isn't smaller than the original. |
| **50%** | Scales image dimensions down to 50%. |
| **Rotate** | Rotates 90° clockwise (respecting EXIF orientation). |
| **Grayscale** | Converts the image to black and white. |
| **Metadata** | Strips EXIF, camera, and GPS location metadata. |
| **Mirror** | Flips the image horizontally. |
| **Crop** | Opens the interactive **Crop Editor** window with **Free**, **1:1**, **4:3**, **3:2**, **16:9**, and **9:16** aspect ratios. When dragging multiple images, the chosen crop region is applied proportionally to all of them. |
| **Trim edges** | Automatically detects and cuts away plain solid borders around screenshots or scans (skipped if the photo has no plain border). |
| **Square** | Smart-crops to a `1:1` square centered on the most visually salient region of the image. |
| **Merge PDF** | *(Shown when dragging 2+ images)* Combines all dragged images into a single `Merged.pdf`. |

### PDF Tools
| Tool | Description |
|---|---|
| **Merge** | *(Shown when dragging 2+ PDFs)* Combines multiple PDFs into a single `Merged.pdf`. |
| **Split** | Splits a multi-page PDF into separate single-page files (`name page 1.pdf`, `name page 2.pdf`, …). |
| **Rotate** | Rotates every page in the PDF by 90° clockwise. |
| **Metadata** | Removes title, author, subject, keywords, creator, producer, and XMP metadata streams. |
| **Page numbers** | Stamps `1 / N` page numbers centered at the bottom of every page. |
| **Reverse** | Reverses the page order of the PDF from last to first. |

### Video Tools
| Tool | Description |
|---|---|
| **Compress** | Downscales to max 1080p and re-encodes with H.264 (`CRF 28`, `+faststart`) for smaller file size. |
| **720p** | Resizes video height to 720p while preserving aspect ratio. |
| **Mute** | Strips the audio track instantly without re-encoding the video stream (`-c:v copy -an`). |
| **Rotate** | Rotates the video 90° clockwise. |
| **Thumbnail** | Extracts a high-quality `.jpg` frame from the midpoint of the video. |
| **2× speed** | Doubles playback speed (`2×`) for both video and audio while preserving pitch. |

### Audio Tools
| Tool | Description |
|---|---|
| **Compress** | Converts and compresses audio to a 128 kbps MP3. |
| **Normalize** | Normalizes loudness to broadcast standard (`-16 LUFS` via FFmpeg `loudnorm`). |
| **Mono** | Mixes stereo/multi-channel audio down to a single mono channel. |
| **Denoise** | Reduces background hiss and noise using FFT spectral denoising (`afftdn`). |
| **Trim silence** | Trims leading and trailing silence from the recording. |
| **1.5× speed** | Speeds up playback by `1.5×` while keeping the original voice/music pitch (`atempo=1.5`). |
| **Fade** | Adds a smooth 2-second fade-in at the start and fade-out at the end. |

### Document, Spreadsheet & Presentation Tools
| Tool | Description |
|---|---|
| **Merge PDF** | *(Shown when dragging 2+ files)* Converts and combines multiple Office files into a single `Merged.pdf`. |
| **Pages / Slides** | Exports every page or slide as a separate `.png` image (requires LibreOffice + Poppler). |

---

## 3. File Actions Wheel (`Alt`)

Tap **Alt** while dragging any file, folder, or multi-selection:

| Action | Description |
|---|---|
| **Copy path** | Copies the full absolute path(s) to the clipboard as plain text (one line per item). |
| **Copy** | Copies the actual file(s) or folder(s) to the Windows clipboard (`CF_HDROP`), ready to paste anywhere with `Ctrl+V`. |
| **Copy to** | Opens a folder picker and copies all dragged items into the chosen folder (auto-renaming on collision). |
| **Move to** | Opens a folder picker and moves all dragged items into the chosen folder (works across different drives). |
| **Terminal** | Opens the item's folder directly in **Windows Terminal** (`wt.exe`), falling back to **Command Prompt** (`cmd.exe`). |
| **Explorer** | Opens File Explorer and highlights the dragged item. |

---

## 4. System Tray & Sound Effects

Shiftwheel runs quietly in the Windows system tray with no main window:
- **Enabled**: Toggle the drag-and-drop hook on or off (`Shift+Ctrl+Alt` detection).
- **Sounds**: Toggle synthesized UI sound effects on or off (persisted in `%APPDATA%\Shiftwheel\settings.json`).
- **Start with Windows**: Automatically launch Shiftwheel hidden in the tray at login.
- **Quit**: Exit Shiftwheel completely.

**Synthesized Audio Feedback:**
- **Open / Close**: Gentle rising (`620 → 940 Hz`) and falling (`840 → 540 Hz`) sine tones when the wheel opens or closes.
- **Hover Tick**: Rate-limited soft triangle click as you sweep across segments.
- **Wheel Switch**: Two-note chime when switching between `Shift`, `Ctrl`, and `Alt` wheels.
- **Select / Cancel / Done / Error**: Distinct tonal cues on drop, cancellation, long-job completion (>1.5 s), and error.

---

## Requirements

Everything runs locally on your PC. Plain image conversions, image tools, and PDF tools work out of the box with zero external dependencies. For Office documents, audio/video, and PDF rasterization, Shiftwheel automatically locates these helper tools in standard install paths, `winget` packages, or `PATH`:

| Program | Used for | Install Command |
|---|---|---|
| [LibreOffice](https://www.libreoffice.org/download/) | Documents, spreadsheets, presentations, `PDF → DOCX/TXT` | `winget install TheDocumentFoundation.LibreOffice` |
| [FFmpeg](https://www.gyan.dev/ffmpeg/builds/) | Audio, video, `BMP`/`HEIC` images | `winget install Gyan.FFmpeg` |
| [Poppler](https://github.com/oschwartz10612/poppler-windows/releases) | `PDF → JPG/PNG`, OCR for scanned PDFs, document `Pages/Slides` | `winget install oschwartz10612.Poppler` |
| Windows OCR | `Image → TXT`, scanned `PDF → TXT` | Built into Windows 10/11 (requires at least one installed OCR language pack, e.g. English) |

If a required helper program is missing, Shiftwheel displays a notification with the exact `winget` command to install it.

---

## Installing for Everyday Use

For development or building from source (requires Node.js 20+):

```bash
npm install
npm run dist
```

The installer is created at `dist/Shiftwheel Setup <version>.exe`:
- Installs per-user in `%LOCALAPPDATA%\Programs\Shiftwheel` (no administrator privileges required).
- Adds a Start Menu shortcut and registers the **Convert with Shiftwheel** Explorer context menu.
- Enables autostart on first run (`--hidden` straight to the system tray).

---

## Light on CPU and RAM

Shiftwheel is engineered to stay in the background all day with minimal overhead (~75 MB private RAM across 3 processes, ~0.4% of one CPU core when idle):
- **No GPU Process**: Renders on CPU (`app.disableHardwareAcceleration()`), dropping Chromium's GPU footprint from ~80 MB to ~15 MB.
- **In-Process Network Service**: Runs `NetworkServiceInProcess` since Shiftwheel never uses the network.
- **Lazy-Loaded Native Modules**: `sharp` (`libvips`) and `pdf-lib` are loaded on demand only when the first conversion runs.
- **On-Demand Windows**: Both the progress HUD and the Crop Editor window are created only when needed and destroyed immediately after use.
- **Auto-Closing Audio Engine**: The `AudioContext` closes 1.5 seconds after the last sound effect and recreates in ~25–55 ms when needed.

---

## Development Commands

| Command | Purpose |
|---|---|
| `npm run dev` | Run from source with hot-reload whenever an app file is saved (automatically closes any installed tray instance first). |
| `npm run install-local` | Build the NSIS installer into `dist/install-<timestamp>` and silently install + launch the updated tray app. |
| `npm test` | Run `node --check`, verify inline `<script>`/`<style>` CSP SHA-256 hashes, run hotkey state-machine tests, and run converter/tool tests. |
| `node test/check.js --fix` | Recompute and update the CSP SHA-256 hashes in `overlay.html`, `hud.html`, and `crop.html` after editing inline styles or scripts. |
| `npm run dist` | Build the standalone Windows installer in `dist/`. |

---

## Notes & Behavior Details

- **Smart File-Drag Probe**: When a mouse drag + **Shift**/**Ctrl**/**Alt** tap is detected, the overlay probes silently (`opacity: 0`) and only reveals the wheel and plays the open sound once Windows OLE (`dragenter`/`dragover`) confirms actual files (`Files`) are being dragged. Selecting text, dragging windows/sliders, or clicking while holding Shift outside a file drag will not trigger the wheel.
- **Modifier Hold vs. Tap**: Holding `Ctrl` or `Shift` *before* starting a drag (standard Explorer copy/move behavior) will not trigger the wheel unless you tap the key cleanly or press it during the drag.
- **Resilient Key Tracking**: Windows occasionally drops `keyup` events (e.g. pressing Left Shift and releasing Right Shift, or `Alt+Tab`). Shiftwheel's state machine in `hotkeys.js` automatically recovers from lost key-ups and self-heals the global hook after sleep or screen lock.
- **Administrator Windows**: Due to Windows UIPI security restrictions, a non-elevated tray app cannot receive input hooks while the cursor is hovering over a window running as Administrator.
- **Non-Destructive Drop**: Even though tapping `Shift` tells Explorer to show a "Move" cursor badge, Shiftwheel always sets `dropEffect = 'copy'` so your original file is never moved or deleted when dropped on the wheel.
