<p align="center">
  <img src="assets/icon.svg" width="80" height="80" alt="Shiftwheel">
</p>

<h1 align="center">Shiftwheel</h1>

<p align="center">
  <b>Instant radial context wheel for file conversions, tools, and actions on Windows.</b><br>
  <i>Drag any file, tap a key, and drop it on a slice.</i>
</p>

<p align="center">
  <a href="https://github.com/Hesyitena/shiftwheel/releases"><img src="https://img.shields.io/github/v/release/Hesyitena/shiftwheel?style=flat-square&color=crimson" alt="Release"></a>
  <img src="https://img.shields.io/badge/platform-Windows%2010%20%2F%2011-blue?style=flat-square" alt="Windows 10/11">
  <img src="https://img.shields.io/badge/architecture-Tauri%20v2%20%2B%20Rust-orange?style=flat-square" alt="Tauri v2 + Rust">
  <img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="License">
</p>

<p align="center">
  Shiftwheel lives quietly in your Windows system tray. When you drag a file anywhere on your desktop or File Explorer and tap a modifier key, a sleek radial wheel blooms directly under your cursor. Drop the file onto a segment to trigger fast local conversions, image tools, or file actions.
</p>

<p align="center">
  <b>100% Local & Private.</b> No cloud uploads, no account required, zero telemetry, zero background network calls.
</p>

---

## ⚡ The Three Wheels

| Key | Wheel | Capabilities |
|---|---|---|
| <kbd>Shift</kbd> | **Convert** | Convert between Images, PDF, Documents, Spreadsheets, Presentations, Audio, Video, and Text |
| <kbd>Ctrl</kbd> | **Tools** | Smart Compression, 50% scale, 1080p fit, Square 1:1, Visual Crop Editor, Rotate, Flip, Grayscale, Trim borders, PDF Merge/Split, Audio Speed/Mute, Video Trim/Audio extraction |
| <kbd>Alt</kbd> | **Actions** | Copy Path, Copy file to clipboard (`CF_HDROP`), Copy/Move to folder dialog, Open in Windows Terminal / CMD, Reveal in Explorer |

> **Mid-Drag Switching:** Tap another key while holding your dragged file to swap between Convert, Tools, and Actions instantly.
>
> **Canceling:** Release outside the wheel, drop onto the center hole, or press <kbd>Esc</kbd>. Original files are never modified or overwritten.

---

## ✨ Key Features & UX

- **Sleek "Dot" Design**: Minimalist dark radial wheel with high-contrast dot-pattern highlight and embedded offline typography (*Doto* & *Space Mono*).
- **Live Drag Thumbnail**: Center preview displays real file thumbnail, filename, file count, and destination format in real time during drag hover.
- **Buttery-Smooth Pointer**: Decelerated Apple-style pointer rotation (`cubic-bezier(0.16, 1, 0.3, 1)`) with shortest-path angular delta tracking so rotation never jerks or snaps across the 12 o'clock boundary.
- **Interactive Crop Editor**: Dedicated window for visual multi-aspect cropping (`Free`, `1:1`, `4:3`, `3:2`, `16:9`, `9:16`) applying seamlessly to single or batch images.
- **Non-Destructive Workflows**: Converted files are saved right next to the original (`photo (compressed).jpg`, `name (1).ext`) with automatic collision resolution. Intercepts drops with copy effect to ensure Windows Explorer never deletes or moves source files unintentionally.
- **Instant Explorer Sync**: Dispatches native Windows Shell notifications (`SHChangeNotify`) so new files appear instantly in File Explorer without pressing <kbd>F5</kbd>.
- **Unobtrusive HUD Card**: Compact progress bar floats in the bottom-right corner over the taskbar. On completion, it reveals an action card with thumbnail, file size change, and one-click *Open File* / *Open Folder* buttons.
- **Synthesized WebAudio SFX**: Dynamic sound effects generated via WebAudio oscillators with intelligent volume levels and automatic context hibernation after 1.5s idle.

---

## 🔄 Supported Conversions (<kbd>Shift</kbd>)

| Category | Input Formats | Target Outputs | Engine / Backend |
|---|---|---|---|
| **Images** | `png`, `jpg`, `jpeg`, `webp`, `gif`, `tiff`, `avif`, `bmp`, `heic`, `svg` | `JPG`, `PNG`, `WEBP`, `AVIF`, `TIFF`, `GIF`, `BMP`, `PDF`, `TXT` | Rust native (`image` crate + Rayon parallel batches); `heic`/`avif`/`svg` and `AVIF` output via FFmpeg; `TXT` via Windows OCR |
| **PDF** | `pdf` | `DOCX`, `TXT`, `JPG`, `PNG` | LibreOffice; Poppler (`pdftotext`, `pdftoppm`) when installed |
| **Documents** | `doc`, `docx`, `odt`, `rtf`, `txt`, `html`, `md` | `PDF`, `DOCX`, `ODT`, `RTF`, `TXT`, `HTML` | LibreOffice headless |
| **Spreadsheets** | `xls`, `xlsx`, `ods`, `csv` | `PDF`, `XLSX`, `ODS`, `CSV` | LibreOffice headless |
| **Presentations** | `ppt`, `pptx`, `odp` | `PDF`, `PPTX`, `ODP` | LibreOffice headless |
| **Audio** | `mp3`, `m4a`, `wav`, `flac`, `ogg`, `opus`, `aiff`, `wma`, `aac` | `MP3`, `M4A`, `WAV`, `FLAC`, `OGG`, `OPUS`, `AIFF` | FFmpeg |
| **Video** | `mp4`, `mov`, `mkv`, `webm`, `avi`, `wmv`, `m4v` | `MP4`, `MOV`, `MKV`, `WEBM`, `GIF`, `MP3` | FFmpeg |
| **Code & Text** | `json`, `log`, `xml`, `yaml`, source code files | `PDF`, `DOCX`, `ODT`, `TXT` | LibreOffice headless |

*Note: Slices for formats a file already possesses are automatically omitted.*

---

## 🛠️ Tools (<kbd>Ctrl</kbd>)

- **Images**:
  - **Compress**: Smart compression balancing quality and size (max 2560px; skipped if result is larger).
  - **50% / Fit 1080p**: Downscale dimensions while preserving aspect ratio.
  - **Square (1:1)**: Center crop to exact 1:1 ratio.
  - **Crop Editor**: Interactive multi-preset crop canvas.
  - **Rotate & Flip**: 90° clockwise rotation and horizontal mirror flip.
  - **Grayscale**: Clean desaturation.
  - **Trim Border**: Automatically detects and trims uniform margin borders (ideal for screenshots).
  - **To PDF**: Combines multiple dragged images into a single multi-page PDF document.
- **PDF**:
  - **Compress**: Optimized PDF stream compression.
  - **Extract Pages**: Splinters PDF into individual single-page documents.
  - **Extract Images**: Extracts rendered pages as PNGs (via Poppler).
  - **Merge**: Merges multiple dropped PDFs into one chronological file.
- **Audio & Video**:
  - **Audio**: Trim silence edges, fade in/out, 2× playback speed (preserved pitch), downmix to mono.
  - **Video**: Remove audio (instant stream copy without re-encoding), extract audio track to `.mp3`, 2× playback speed, scale to 720p, reverse video.

---

## 📂 File Actions (<kbd>Alt</kbd>)

Works seamlessly across files, folders, and mixed selections:
- **Copy Path**: Copies absolute paths to the clipboard (multi-file selections separated by newlines).
- **Copy Files**: Places files onto the Windows clipboard (`CF_HDROP`) for instant <kbd>Ctrl</kbd>+<kbd>V</kbd> pasting.
- **Copy To / Move To**: Prompts the native Windows folder picker and moves/copies files with conflict safety.
- **Terminal**: Opens Windows Terminal (`wt.exe`) or `cmd.exe` directly at the item's directory.
- **Explorer**: Selects and highlights the item in File Explorer.

---

## 🧩 Helper Tools

Basic image conversions, image tools, PDF merge/split, and Windows file actions run **100% natively in Rust** without any external dependencies. For specialized media and office conversions, Shiftwheel automatically detects installed helper tools on your `PATH` or standard installation directories:

| Helper Tool | Used For | Fast WinGet Installation |
|---|---|---|
| [FFmpeg](https://www.gyan.dev/ffmpeg/builds/) | Audio & video conversion/tools, `AVIF` output, `HEIC`/`SVG` input | `winget install Gyan.FFmpeg` |
| [LibreOffice](https://www.libreoffice.org/download/) | Office docs (`DOCX`, `XLSX`, `PPTX` to `PDF`), `PDF → DOCX` | `winget install TheDocumentFoundation.LibreOffice` |
| [Poppler](https://github.com/oschwartz10612/poppler-windows) *(optional)* | High-fidelity `PDF → TXT` and multi-page `PDF → PNG/JPG` | `winget install oschwartz10612.Poppler` |
| Windows OCR | Offline text extraction from images (`Image → TXT`) | Built into Windows 10/11 |

*When a slice requires a helper tool that isn't installed yet, the segment is dimmed and clearly indicates which tool is needed.*

---

## 🚀 Benchmark & Architecture

Migrating from Electron to **Tauri v2 + Rust** resulted in massive performance gains:

| Metric | Legacy Electron | Tauri v2 + Rust | Efficiency Gain |
|---|---|---|---|
| **Installer Size** | **103.58 MB** | **2.25 MB** | **-97.8%** ⚡ |
| **Installed Binary** | **~280 MB** | **4.8 MB** | **-98.3%** |
| **Idle Memory (RAM)** | **~85 MB** | **~28 MB** | **-67%** |
| **Startup Time** | **~1.8s** | **< 300ms** | **6× Faster** |

- **Low-Level Native Windows Hooks**: Low-level `WH_MOUSE_LL` and `WH_KEYBOARD_LL` Windows hooks run in a dedicated background OS thread without Node.js overhead.
- **Hardware-Accelerated WebView2**: Uses the evergreen Windows system WebView2 runtime for crisp, low-overhead rendering.
- **Rayon Multithreading**: Batch image processing harnesses all available CPU cores in parallel.

---

## 📥 Installation

### Download Prebuilt Setup
1. Go to the [Releases](https://github.com/Hesyitena/shiftwheel/releases) page.
2. Download `Shiftwheel_1.1.0_x64-setup.exe` (~2.25 MB) or standalone `shiftwheel.exe`.
3. Run the setup — it installs per-user into `%LOCALAPPDATA%\Programs\Shiftwheel` (no administrator privileges or UAC prompts required).
4. Shiftwheel will appear in your system tray ready to use!

---

## 🔨 Building from Source

### Prerequisites
- [Rust](https://rustup.rs/) (1.80+)
- [Node.js](https://nodejs.org/) (20+)

### Commands

```bash
# 1. Clone repository
git clone https://github.com/Hesyitena/shiftwheel.git
cd shiftwheel

# 2. Install dev dependencies
npm install

# 3. Run in development mode
npm run dev

# 4. Build standalone release binary
npm run build

# 5. Build complete NSIS installer
npm run dist
```

Built artifacts are output to `src-tauri/target/release/` and copied to `dist/`.

---

## 📜 License & Credits

Developed with ❤️ by [Hesyitena](https://github.com/Hesyitena).  
Licensed under the [MIT License](LICENSE).
