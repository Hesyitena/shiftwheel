# Shiftwheel

Konversi file ala [Tangerine](https://tangerineformac.com/) (macOS) untuk Windows 11. Mulai drag file dari Explorer, **ketuk** sebuah tombol, lalu roda muncul di sekitar kursor dan tetap terbuka sampai file di-drop:

| Ketuk saat drag | Roda | Contoh isi |
|---|---|---|
| **Shift** | Konversi | gambar → JPG, PNG, WEBP, AVIF, TIFF, GIF, BMP, PDF |
| **Ctrl** | Tools | kompres, 50%, putar, hitam putih, hapus metadata, gabung jadi PDF |
| **Alt** | Aksi file | salin path, salin, salin ke…, pindah ke…, terminal, tampilkan di Explorer |

Isi roda mengikuti tipe file yang di-drag. Bagian tengah roda menunjukkan apa yang sedang dibawa (misalnya "2 gambar"), lalu nama dan keterangan opsi yang sedang disorot. Bar kecil di bawah roda menunjukkan roda yang aktif; ketuk tombol lain untuk berganti roda. Lepas file di tengah ("Batal") atau di luar roda, atau tekan **Esc**, untuk batal. Hasil disimpan di folder yang sama dengan file asal.

Kalau prosesnya lebih dari setengah detik, kartu progres kecil muncul di pojok kanan bawah (audio/video dengan persentase). Setelah selesai muncul notifikasi, misalnya "laporan.pdf siap"; klik untuk membuka foldernya.

Shiftwheel berjalan di system tray tanpa jendela. Menu tray berisi **Aktif**, **Suara**, **Jalankan saat Windows menyala**, dan **Keluar**.

**Suara.** Efek suara pendek saat roda muncul, saat pindah segmen, saat ganti roda (Shift/Ctrl/Alt), saat file di-drop atau dibatalkan, dan saat proses gagal. Chime "selesai" hanya berbunyi untuk proses yang lebih dari 1,5 detik, supaya konversi instan tidak ramai. Semua suara disintesis langsung (WebAudio), tanpa file audio. Matikan lewat **Suara** di menu tray; pilihannya tersimpan.

## Kebutuhan

Semua konversi berjalan lokal di PC, tidak ada file yang di-upload. Program bantu berikut dipakai sesuai kebutuhan (Shiftwheel mencarinya di folder instalasi umum, paket winget, dan PATH):

| Program | Untuk | Install |
|---|---|---|
| [LibreOffice](https://www.libreoffice.org/download/) | dokumen, spreadsheet, presentasi, PDF → DOCX/TXT | `winget install TheDocumentFoundation.LibreOffice` |
| [ffmpeg](https://www.gyan.dev/ffmpeg/builds/) | audio, video, gambar BMP/HEIC | `winget install Gyan.FFmpeg` |
| [Poppler](https://github.com/oschwartz10612/poppler-windows/releases) | PDF → JPG/PNG, OCR PDF hasil scan | `winget install oschwartz10612.Poppler` |
| OCR Windows | gambar → TXT, PDF hasil scan → TXT | bawaan Windows 10/11, aktif kalau ada bahasa dengan fitur OCR (bahasa Inggris sudah cukup untuk teks Indonesia) |

Konversi gambar biasa dan semua tools gambar/PDF tidak butuh program tambahan. Kalau program bantu belum terpasang, notifikasi menyebutkan perintah install-nya.

Untuk development: Node.js 20+.

## Instalasi untuk pemakaian harian

```bash
npm install
npm run dist
```

Installer ada di `dist/Shiftwheel Setup <versi>.exe`. Jalankan installer itu:

- Terinstal per-user di `%LOCALAPPDATA%\Programs\Shiftwheel`, tanpa hak admin.
- Shortcut dibuat di Start Menu (tidak di desktop).
- App langsung jalan setelah instalasi dan muncul notifikasi "Shiftwheel berjalan di tray".
- Saat pertama jalan, autostart aktif otomatis (`--hidden`: langsung ke tray, tanpa notifikasi). Bisa dimatikan dari menu tray.
- Klik kanan file di Explorer → **Show more options** → **Konversi dengan Shiftwheel** → PDF / DOCX / TXT / JPG.

Uninstall lewat Settings → Apps. Uninstaller menutup Shiftwheel yang sedang jalan, lalu menghapus entri autostart dan menu klik kanan. Pengaturan di `%APPDATA%\Shiftwheel` tidak ikut dihapus.

## Ringan di CPU dan RAM

Shiftwheel jalan terus di latar belakang, jadi dibuat sehemat mungkin:

- **Tanpa GPU.** Roda cukup dirender CPU; proses GPU Chromium turun dari ~80 MB ke ~15 MB.
- **Tanpa proses jaringan.** Network service digabung ke proses utama (Shiftwheel tidak pernah memakai internet).
- **Library berat dimuat saat dibutuhkan.** sharp (libvips) dan pdf-lib baru dimuat saat konversi pertama.
- **Jendela progres dibuat saat perlu** dan dihancurkan lagi setelah selesai.
- **Audio tidur saat diam.** Mesin suara disuspend 1,5 detik setelah bunyi terakhir dan ditutup setelah 30 detik.
- **Hook tanpa `mousemove`.** Gerakan mouse sehari-hari tidak diproses; jarak drag dihitung sekali saat Shift/Ctrl/Alt diketuk.
- LibreOffice, ffmpeg, dan OCR hanya jalan selama ada file yang diproses.

## Development dan mencoba update

| Perintah | Kapan dipakai |
|---|---|
| `npm run dev` | Mencoba perubahan langsung. Shiftwheel jalan dari source dan **restart sendiri setiap file app disimpan**. Versi terinstal ditutup dulu (keduanya tidak bisa jalan bersamaan); buka lagi dari Start Menu setelah selesai. Berhenti dengan Ctrl+C atau **Keluar** di menu tray. |
| `npm run pasang` | Perubahan sudah oke dan mau dipakai harian. Build installer lalu memasangnya diam-diam di PC ini (~1 menit). Versi lama ditutup, versi baru langsung jalan di tray, pengaturan tetap. |
| `npm test` | `node --check`, cek hash CSP, dan tes converter/tools. Tes yang butuh LibreOffice/ffmpeg/Poppler dilewati kalau programnya tidak ada. |
| `npm run dist` | Hanya membuat installer di `dist/` (misalnya untuk dipasang di PC lain). |

Alurnya: edit kode → coba dengan `npm run dev` → `npm test` → commit dan push → `npm run pasang`.

Kalau `npm run dist` gagal dengan `EBUSY ... app.asar`, berarti ada app Electron lain (editor atau Claude desktop yang sedang membuka folder ini) yang memegang `dist/win-unpacked/resources/app.asar`. Tutup app itu atau hapus `dist/win-unpacked`. `npm run pasang` tidak kena masalah ini karena build di folder sementara.

Jalankan `npm run pasang` dari terminal Windows biasa (Windows Terminal, PowerShell, cmd), **bukan** dari terminal di dalam aplikasi Claude desktop. Proses yang dijalankan dari dalam Claude desktop ikut tervirtualisasi (MSIX): tulisan ke `%LOCALAPPDATA%`/`%APPDATA%` dialihkan ke folder privat Claude, jadi Shiftwheel akan terpasang di tempat yang tidak dilihat Windows.

Setelah mengedit `<style>` atau `<script>` di `overlay.html` atau `hud.html`, perbarui hash CSP dengan `node test/check.js --fix` (kalau lupa, overlay tampil tanpa gaya dan `npm test` gagal).

Kategori dan format tujuan diatur di satu tempat: `CATEGORIES` di `converter.js`. Tools ada di `TOOLS` di `tools.js`, aksi file di `ACTIONS` di `main.js`. Roda dibangun dari ketiganya.

Ikon: `assets/icon.png` (64×64). `npm run dist` otomatis membuat `build/icon.ico` (16, 32, 48, 256) lewat `scripts/make-ico.js`.

## Roda konversi (Shift)

| Tipe file | Sumber | Tujuan | Lewat |
|---|---|---|---|
| Gambar | png, jpg, webp, gif, tiff, avif, bmp, heic, svg | JPG, PNG, WEBP, AVIF, TIFF, GIF, BMP, PDF, TXT | sharp, pdf-lib, OCR Windows (BMP/HEIC lewat ffmpeg) |
| PDF | pdf | DOCX, TXT, JPG, PNG | LibreOffice, pdftoppm |
| Dokumen | doc, docx, odt, rtf, txt, html | PDF, DOCX, ODT, RTF, TXT, HTML | LibreOffice |
| Spreadsheet | xls, xlsx, ods, csv | PDF, XLSX, ODS, CSV | LibreOffice |
| Presentasi | ppt, pptx, odp | PDF, PPTX, ODP | LibreOffice |
| Audio | mp3, m4a, wav, flac, ogg, opus, aiff, wma, aac | MP3, M4A, WAV, FLAC, OGG, OPUS, AIFF | ffmpeg |
| Video | mp4, mov, mkv, webm, avi, wmv, m4v | MP4, MOV, MKV, WEBM, GIF, MP3 | ffmpeg |

- Gambar → JPG: orientasi mengikuti EXIF, transparansi diganti putih, kualitas 92. Animasi GIF ↔ WEBP dipertahankan.
- Gambar → TXT: teks diambil dengan OCR bawaan Windows, offline. Gambar kecil diperbesar 2× dulu supaya tulisan kecil terbaca. Gambar tanpa tulisan ditolak, tidak membuat file kosong.
- Gambar → PDF: satu halaman dengan lebar 595 pt (A4), tinggi mengikuti rasio gambar.
- PDF → JPG/PNG: semua halaman pada 150 DPI. Satu halaman jadi `nama.jpg`; banyak halaman jadi `nama 1.jpg`, `nama 2.jpg`, dst.
- PDF → TXT: LibreOffice meng-import PDF sebagai frame teks yang diabaikan ekspor TXT biasa, jadi Shiftwheel mengambil teks lewat FODT lalu mengurutkannya sesuai posisi di halaman. Teks biasa hasilnya rapi; tata letak kolom/tabel jadi baris berurutan kiri→kanan. PDF hasil scan atau foto (tanpa lapisan teks) otomatis dibaca dengan OCR Windows: tiap halaman dirender 200 DPI lalu dikenali, antarhalaman dipisah baris kosong.
- PDF → DOCX: teks masuk ke text box per baris (perilaku import PDF LibreOffice), jadi bisa dibaca dan diedit tapi tata letaknya tidak mengalir seperti dokumen Word biasa.
- Video → MP4/MOV/MKV: H.264 + AAC (CRF 20). WEBM: VP9 + Opus. GIF: 12 fps, lebar maks 480 px.
- File yang sudah berformat tujuan dilewati ("Sudah berformat X").
- Hasil tidak pernah menimpa file lain: `nama.pdf`, lalu `nama (1).pdf`, `nama (2).pdf`, dst.

## Roda tools (Ctrl)

Hasil disimpan sebagai file baru, misalnya `foto (dikompres).jpg`. File asli tidak diubah.

| Tipe file | Tools |
|---|---|
| Gambar | **Kompres** (maks 2560 px, kualitas ~75; dilewati kalau hasilnya tidak lebih kecil), **50%**, **Putar** 90°, **Hitam Putih**, **Metadata** (hapus EXIF/lokasi), **Gabung PDF** (≥ 2 gambar jadi `Gabungan.pdf`) |
| PDF | **Gabung** (≥ 2 PDF jadi `Gabungan.pdf`), **Pisah** per halaman (`nama hal 1.pdf`, …), **Putar** semua halaman 90°, **Metadata** (hapus judul/penulis/XMP) |
| Video | **Kompres** (maks 1080p, CRF 28), **720p**, **Bisukan**, **Putar** 90° |
| Audio | **Kompres** (MP3 128 kbps), **Normalisasi** (loudnorm −16 LUFS), **Mono** |

Dokumen, spreadsheet, dan presentasi belum punya tools.

## Roda aksi file (Alt)

Berlaku untuk file apa pun, termasuk folder dan pilihan campuran:

- **Salin path**: path lengkap ke clipboard sebagai teks, satu baris per item.
- **Salin**: file ke clipboard, siap di-paste di Explorer.
- **Salin ke / Pindah ke**: pilih folder tujuan. Nama yang bentrok jadi `nama (1)`.
- **Terminal**: buka folder (atau folder tempat file berada) di Windows Terminal, atau Command Prompt kalau Windows Terminal tidak ada.
- **Explorer**: tampilkan item di Explorer.

## Catatan

- **Deteksi drag bersifat heuristik.** Windows tidak punya API global untuk "file sedang di-drag". Shiftwheel menganggap drag sedang terjadi kalau tombol kiri mouse ditahan dan kursor bergerak lebih dari 20 px. Roda juga bisa muncul kalau Shift/Ctrl/Alt diketuk saat menyeleksi teks atau memindah jendela. Roda hilang saat tombol mouse dilepas atau Esc ditekan, dan tidak ada yang diproses kalau tidak ada file yang di-drop.
- **Roda hanya terbuka kalau tombol diketuk *selama* drag.** Menahan Ctrl/Shift sebelum mulai drag (kebiasaan copy/move di Explorer) tidak membuka roda.
- **Isi roda ditebak dari tipe MIME** yang diberi Windows saat file masuk ke roda. Untuk ekstensi yang tidak dikenal Windows, roda konversi menampilkan format umum (PDF, DOCX, TXT, JPG, PNG, MP3, MP4). Validasi akhir tetap memakai ekstensi file, jadi pilihan yang tidak cocok ditolak dengan pesan yang jelas.
- **Alt** saat drag kadang membuat app yang sedang aktif (mis. Explorer) mengaktifkan menu bar setelah drop. Tekan Alt atau Esc sekali lagi untuk menutupnya.
- **HEIC** dibaca lewat ffmpeg dan belum diuji dengan foto iPhone asli.
- **Tidak bekerja di atas jendela Administrator.** Hook global dari app biasa tidak menerima input saat kursor berada di atas jendela yang berjalan sebagai Administrator (batasan UIPI Windows).
- **Peringatan SmartScreen.** Installer tidak di-code-sign, jadi Windows menampilkan "Windows protected your PC". Klik **More info** → **Run anyway**.
- **Antivirus.** Hook keyboard/mouse global bisa dicurigai sebagian antivirus. Hook Shiftwheel hanya membaca tombol Shift, Ctrl, Alt, Esc, dan tombol kiri mouse beserta posisi kursor. Shiftwheel tidak merekam ketikan, tidak menyimpan input apa pun, dan tidak mengakses jaringan.
- **Menu klik kanan di Windows 11** ada di bawah "Show more options" (menu klasik), karena menu baru Windows 11 hanya untuk app yang di-package dengan identitas MSIX.
- Shift atau Ctrl saat drag membuat Explorer menganggapnya "move" atau "copy". Shiftwheel selalu membalas sebagai "copy", jadi file asli tidak pernah dipindah atau dihapus oleh drop ke roda.
