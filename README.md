# Shiftwheel

Konversi file ala Tangerine (macOS) untuk Windows 11. Drag file dari Explorer, tahan **Shift**, lalu menu melingkar muncul di sekitar kursor:

```
        DOCX
  TXT  (batal)  JPG
        PDF
```

Lepas file di salah satu segmen. File langsung dikonversi dan hasilnya disimpan di folder yang sama dengan file asal. Lepas di tengah ("batal") atau di luar menu berarti batal. Setelah selesai muncul notifikasi Windows. Klik notifikasinya untuk membuka folder hasil.

Shiftwheel berjalan di system tray tanpa jendela. Menu tray berisi **Aktif**, **Jalankan saat Windows menyala**, dan **Keluar**.

## Kebutuhan

- Windows 10/11 64-bit
- [LibreOffice](https://www.libreoffice.org/download/) untuk konversi dokumen dan PDF → DOCX/TXT
- (Opsional) [Poppler](https://github.com/oschwartz10612/poppler-windows/releases) dengan `pdftoppm` di PATH, untuk PDF → JPG

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

## Development

```bash
npm start        # jalankan dari source (autostart tidak didaftarkan saat dev)
npm test         # node --check + cek hash CSP + tes converter
```

Setelah mengedit `<style>` atau `<script>` di `overlay.html`, perbarui hash CSP dengan `node test/check.js --fix`.

Ikon: `assets/icon.png` (64×64). `npm run dist` otomatis membuat `build/icon.ico` (16, 32, 48, 256) lewat `scripts/make-ico.js`.

## Matriks konversi

| Dari \ Ke | PDF | DOCX | TXT | JPG |
|---|---|---|---|---|
| doc, docx, odt, rtf, txt, html | LibreOffice | LibreOffice | LibreOffice | – |
| pdf | (sama) | LibreOffice | LibreOffice | pdftoppm (halaman 1) |
| png, jpg, webp, gif, tiff, avif | pdf-lib | – | – | sharp |

- PDF → TXT: LibreOffice meng-import PDF sebagai frame teks yang diabaikan ekspor TXT biasa, jadi Shiftwheel mengambil teks lewat FODT lalu mengurutkannya sesuai posisi di halaman. Teks biasa hasilnya rapi; tata letak kolom/tabel jadi baris berurutan kiri→kanan. PDF hasil scan (tanpa lapisan teks) ditolak karena tidak ada OCR.
- PDF → DOCX: teks masuk ke text box per baris (perilaku import PDF LibreOffice), jadi bisa dibaca dan diedit tapi tata letaknya tidak mengalir seperti dokumen Word biasa.
- Gambar → PDF: satu halaman dengan lebar 595 pt (A4), tinggi mengikuti rasio gambar.
- Gambar → JPG: orientasi mengikuti EXIF, transparansi diganti putih, kualitas 92.
- File yang sudah berformat tujuan dilewati ("Sudah berformat X").
- Hasil tidak pernah menimpa file lain: `nama.pdf`, lalu `nama (1).pdf`, `nama (2).pdf`, dst.
- Kombinasi bertanda – ditolak dengan pesan yang jelas.

## Catatan

- **Deteksi drag bersifat heuristik.** Windows tidak punya API global untuk "file sedang di-drag". Shiftwheel menganggap drag sedang terjadi kalau tombol kiri mouse ditahan, kursor bergerak lebih dari 20 px, dan Shift ditekan. Akibatnya menu juga bisa muncul di app lain, misalnya saat Shift + seleksi teks. Menu hilang sendiri saat tombol mouse atau Shift dilepas, dan tidak ada yang dikonversi kalau tidak ada file yang di-drop.
- **Tidak bekerja di atas jendela Administrator.** Hook global dari app biasa tidak menerima input saat kursor berada di atas jendela yang berjalan sebagai Administrator (batasan UIPI Windows).
- **Peringatan SmartScreen.** Installer tidak di-code-sign, jadi Windows menampilkan "Windows protected your PC". Klik **More info** → **Run anyway**.
- **Antivirus.** Hook keyboard/mouse global bisa dicurigai sebagian antivirus. Hook Shiftwheel hanya membaca status tombol Shift dan tombol kiri mouse beserta posisi kursor. Shiftwheel tidak merekam ketikan, tidak menyimpan input apa pun, dan tidak mengakses jaringan.
- **Menu klik kanan di Windows 11** ada di bawah "Show more options" (menu klasik), karena menu baru Windows 11 hanya untuk app yang di-package dengan identitas MSIX.
- Kalau Shift ditahan saat drag, Explorer menganggapnya "move". Shiftwheel selalu membalas sebagai "copy", jadi file asli tidak pernah dipindah atau dihapus.
