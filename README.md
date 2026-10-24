# Shiftwheel

A radial drag-and-drop file converter and toolkit for Windows 11. Start dragging a file in Explorer, **tap** a key, and a wheel opens around the cursor. It stays open until you drop the file:

| Tap while dragging | Wheel | Examples |
|---|---|---|
| **Shift** | Convert | image → JPG, PNG, WEBP, AVIF, TIFF, GIF, BMP, PDF, TXT |
| **Ctrl** | Tools | compress, rotate, trim edges, merge into a PDF, denoise audio |
| **Alt** | File actions | copy path, copy, copy to…, move to…, terminal, show in Explorer |

The wheel's contents depend on the type of file you're dragging. The center of the wheel shows what you're carrying (for example "2 images"), then the name and description of the highlighted option. The small bar under the wheel shows the active wheel; tap another key to switch. Drop on the center ("Cancel") or outside the wheel, or press **Esc**, to cancel. Results are saved next to the original file.

If a job takes more than half a second, a small progress card appears in the bottom-right corner (with a percentage for audio/video). When it's done you get a notification such as "report.pdf is ready"; click it to open the folder.

Shiftwheel lives in the system tray with no window. The tray menu has **Enabled**, **Sounds**, **Start with Windows**, and **Quit**.

**Sounds.** Short sound effects when the wheel opens, when you move between segments, when you switch wheels (Shift/Ctrl/Alt), when a file is dropped or cancelled, and when a job fails. The "done" chime only plays for jobs that take more than 1.5 seconds, so instant conversions stay quiet. All sounds are synthesized on the fly (WebAudio), no audio files. Turn them off with **Sounds** in the tray menu; the choice is saved.

## Requirements

Everything runs locally on your PC; no file is ever uploaded. These helper programs are used as needed (Shiftwheel looks for them in common install folders, winget packages, and PATH):

| Program | Used for | Install |
|---|---|---|
| [LibreOffice](https://www.libreoffice.org/download/) | documents, spreadsheets, presentations, PDF → DOCX/TXT | `winget install TheDocumentFoundation.LibreOffice` |
| [ffmpeg](https://www.gyan.dev/ffmpeg/builds/) | audio, video, BMP/HEIC images | `winget install Gyan.FFmpeg` |
| [Poppler](https://github.com/oschwartz10612/poppler-windows/releases) | PDF → JPG/PNG, OCR for scanned PDFs | `winget install oschwartz10612.Poppler` |
| Windows OCR | image → TXT, scanned PDF → TXT | built into Windows 10/11; active when a language with the OCR feature is installed (English works for any Latin-script text) |

Plain image conversions and all image/PDF tools need nothing extra. If a helper program is missing, the notification tells you the command to install it.

For development: Node.js 20+.

## Installing for everyday use

```bash
npm install
npm run dist
```

The installer ends up in `dist/Shiftwheel Setup <version>.exe`. Run it:

- Installs per user in `%LOCALAPPDATA%\Programs\Shiftwheel`, no admin rights needed.
- Adds a Start Menu shortcut (no desktop shortcut).
- Starts the app right away and shows a "Shiftwheel is ready" notification.
- On first run, autostart is turned on (`--hidden`: straight to the tray, no notification). You can turn it off from the tray menu.
- Right-click a file in Explorer → **Show more options** → **Convert with Shiftwheel** → PDF / DOCX / TXT / JPG.

Uninstall from Settings → Apps. The uninstaller closes Shiftwheel if it's running, then removes the autostart entry and the context menu. Settings in `%APPDATA%\Shiftwheel` are kept.

## Light on CPU and RAM

Shiftwheel runs in the background all the time, so it's built to be as cheap as possible:

- **No GPU.** The wheel renders fine on the CPU; Chromium's GPU process drops from ~80 MB to ~15 MB.
- **No network process.** The network service runs inside the main process (Shiftwheel never uses the internet).
- **Heavy libraries load on demand.** sharp (libvips) and pdf-lib only load on the first conversion.
- **The progress window is created when needed** and destroyed again when the job is done.
- **Audio turns off when idle.** The sound engine closes 1.5 seconds after the last sound and is recreated when needed (~25–55 ms; faster than waking a suspended one, which on Windows can take up to ~440 ms).
- **A hook without `mousemove`.** Everyday mouse movement isn't processed; the drag distance is checked once when Shift/Ctrl/Alt is tapped.
- LibreOffice, ffmpeg, and OCR only run while a file is being processed.

Measured idle footprint of the installed app: ~75 MB of private RAM across 3 processes, ~0.4% of one CPU core.

## Development and trying changes

| Command | When to use it |
|---|---|
| `npm run dev` | Try changes right away. Shiftwheel runs from source and **restarts itself every time an app file is saved**. The installed version is closed first (they can't run side by side); reopen it from the Start Menu when you're done. Stop with Ctrl+C or **Quit** in the tray menu. |
| `npm run install-local` | The changes are good and you want to use them every day. Builds the installer and installs it silently on this PC (~1 minute). The old version is closed, the new one starts in the tray, settings are kept. |
| `npm test` | `node --check`, CSP hash check, the key-tap tests, and the converter/tools tests. Tests that need LibreOffice/ffmpeg/Poppler/OCR are skipped when the program isn't there. |
| `npm run dist` | Only builds the installer in `dist/` (for example to install on another PC). |

The loop: edit code → try it with `npm run dev` → `npm test` → commit and push → `npm run install-local`.

If `npm run dist` fails with `EBUSY ... app.asar`, another Electron app (an editor, or Claude desktop with this folder open) is holding `dist/win-unpacked/resources/app.asar`. Close that app or delete `dist/win-unpacked`. `npm run install-local` doesn't hit this because it builds into a fresh `dist/install-<time>` folder each run.

Run `npm run install-local` from a regular Windows terminal (Windows Terminal, PowerShell, cmd), **not** from a terminal inside the Claude desktop app. Processes started inside Claude desktop are virtualized (MSIX): writes to `%LOCALAPPDATA%`/`%APPDATA%` are redirected to Claude's private folder, so Shiftwheel would be installed somewhere Windows can't see.

After editing the `<style>` or `<script>` in `overlay.html`, `hud.html`, or `crop.html`, update the CSP hashes with `node test/check.js --fix` (if you forget, the overlay renders unstyled and `npm test` fails).

Categories and target formats live in one place: `CATEGORIES` in `converter.js`. Tools are in `TOOLS` in `tools.js`, file actions in `ACTIONS` in `main.js`. The wheels are built from those three.

The icon is drawn in code by `scripts/make-ico.js`: a black tile with a white wheel that has one lit segment. The script writes `assets/icon.svg`, `assets/icon.png`, the tray icons `assets/tray-16/24/32.png`, and `build/icon.ico` (16–256 px). Sizes ≤ 32 px use a bolder 4-segment version so they stay crisp. `npm run dist` runs it automatically.

## Convert wheel (Shift)

| File type | From | To | Using |
|---|---|---|---|
| Images | png, jpg, webp, gif, tiff, avif, bmp, heic, svg | JPG, PNG, WEBP, AVIF, TIFF, GIF, BMP, PDF, TXT | sharp, pdf-lib, Windows OCR (BMP/HEIC through ffmpeg) |
| PDF | pdf | DOCX, TXT, JPG, PNG | LibreOffice, pdftoppm |
| Documents | doc, docx, odt, rtf, txt, html | PDF, DOCX, ODT, RTF, TXT, HTML | LibreOffice |
| Spreadsheets | xls, xlsx, ods, csv | PDF, XLSX, ODS, CSV | LibreOffice |
| Presentations | ppt, pptx, odp | PDF, PPTX, ODP | LibreOffice |
| Audio | mp3, m4a, wav, flac, ogg, opus, aiff, wma, aac | MP3, M4A, WAV, FLAC, OGG, OPUS, AIFF | ffmpeg |
| Video | mp4, mov, mkv, webm, avi, wmv, m4v | MP4, MOV, MKV, WEBM, GIF, MP3 | ffmpeg |

- Image → JPG: follows EXIF orientation, transparency becomes white, quality 92. GIF ↔ WEBP animation is kept.
- Image → TXT: text is read with Windows' built-in OCR, offline. Small images are upscaled 2× first so small text reads. Images without writing are rejected instead of producing an empty file.
- Image → PDF: one page 595 pt wide (A4), height following the image's aspect ratio.
- PDF → JPG/PNG: every page at 150 DPI. A single page becomes `name.jpg`; several pages become `name 1.jpg`, `name 2.jpg`, and so on.
- PDF → TXT: LibreOffice imports a PDF as text frames that a normal TXT export ignores, so Shiftwheel reads the text through FODT and sorts it by position on the page. Ordinary text comes out clean; column/table layouts become rows read left → right. Scanned or photographed PDFs (no text layer) are read with Windows OCR automatically: each page is rendered at 200 DPI and recognized, with a blank line between pages.
- PDF → DOCX: text lands in one text box per line (that's how LibreOffice imports PDFs), so it's readable and editable but doesn't flow like a normal Word document.
- Video → MP4/MOV/MKV: H.264 + AAC (CRF 20). WEBM: VP9 + Opus. GIF: 12 fps, max 480 px wide.
- Files already in the target format are skipped ("Already a X file").
- Results never overwrite another file: `name.pdf`, then `name (1).pdf`, `name (2).pdf`, and so on.

## Tools wheel (Ctrl)

Results are saved as new files, for example `photo (compressed).jpg`. The original is never changed.

| File type | Tools |
|---|---|
| Images | **Compress** (max 2560 px, quality ~75; skipped if the result isn't smaller), **50%**, **Rotate** 90°, **Grayscale**, **Metadata** (remove EXIF/location), **Mirror**, **Crop** (opens an editor: drag a selection, pick Free/1:1/4:3/3:2/16:9/9:16, Enter to save; with several images the same crop is applied to all), **Trim edges** (cut away plain borders; skipped when there are none), **Square** 1:1 (smart crop around the most salient part), **Merge PDF** (2+ images into `Merged.pdf`) |
| PDF | **Merge** (2+ PDFs into `Merged.pdf`), **Split** into pages (`name page 1.pdf`, …), **Rotate** every page 90°, **Metadata** (remove title/author/XMP), **Page numbers** ("1 / 5" at the bottom of each page), **Reverse** page order |
| Video | **Compress** (max 1080p, CRF 28), **720p**, **Mute**, **Rotate** 90°, **Thumbnail** (JPG from the middle of the video), **2× speed** |
| Audio | **Compress** (128 kbps MP3), **Normalize** (loudnorm −16 LUFS), **Mono**, **Denoise** (afftdn), **Trim silence** at the start/end, **1.5× speed** (same pitch), **Fade** in/out |
| Documents, spreadsheets, presentations | **Merge PDF** (2+ files into one `Merged.pdf`), **Pages / Slides** (every page as a PNG) |

The document/spreadsheet/presentation tools use LibreOffice; "Pages" also needs Poppler.

## File actions wheel (Alt)

Works on any file, including folders and mixed selections:

- **Copy path**: the full path to the clipboard as text, one line per item.
- **Copy**: the files to the clipboard, ready to paste in Explorer.
- **Copy to / Move to**: pick a destination folder. Clashing names become `name (1)`.
- **Terminal**: open the folder (or the file's folder) in Windows Terminal, or Command Prompt if Windows Terminal isn't installed.
- **Explorer**: show the item in Explorer.

## Notes

- **Drag detection is a heuristic.** Windows has no global API for "a file is being dragged". Shiftwheel assumes a drag is happening when the left mouse button is held and the cursor has moved more than about 20 px. The wheel can also open if you tap Shift/Ctrl/Alt while selecting text or moving a window. It closes when you release the mouse or press Esc, and nothing is processed unless a file is dropped.
- **The wheel only opens when the key is tapped *during* the drag.** Holding Ctrl/Shift before you start dragging (the usual Explorer copy/move habit) doesn't open it. A tap right as the drag starts, before the cursor has moved far enough, waits up to 1.5 s for the drag to begin instead of being ignored.
- **Taps survive lost key events.** Windows sometimes never delivers a key-up (Shift pressed on one side and released on the other, Alt+Tab). Shiftwheel treats a key as a new tap unless it repeated within the last 600 ms, so a lost key-up can never lock the wheel shut (see `hotkeys.js` and `test/hotkeys.test.js`).
- **The wheel's contents are guessed from the MIME type** Windows provides when the file enters the wheel. For extensions Windows doesn't know, the convert wheel shows common formats (PDF, DOCX, TXT, JPG, PNG, MP3, MP4). The final check still uses the file extension, so a mismatched choice is rejected with a clear message.
- **Alt** during a drag sometimes makes the active app (e.g. Explorer) highlight its menu bar after the drop. Press Alt or Esc once more to dismiss it.
- **HEIC** is read through ffmpeg and hasn't been tested with real iPhone photos.
- **Doesn't work over Administrator windows.** A global hook from a regular app gets no input while the cursor is over a window running as Administrator (a Windows UIPI restriction).
- **SmartScreen warning.** The installer isn't code-signed, so Windows shows "Windows protected your PC". Click **More info** → **Run anyway**.
- **Antivirus.** Some antivirus products may be suspicious of a global keyboard/mouse hook. Shiftwheel's hook only reads Shift, Ctrl, Alt, Esc, and the left mouse button plus the cursor position. It doesn't record keystrokes, store any input, or access the network.
- **The context menu on Windows 11** is under "Show more options" (the classic menu), because the new Windows 11 menu is only for apps packaged with an MSIX identity.
- Shift or Ctrl during a drag tells Explorer it's a "move" or "copy". Shiftwheel always answers "copy", so the original file is never moved or deleted by dropping it on the wheel.
