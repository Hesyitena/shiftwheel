// Shiftwheel: proses utama (tray, hook global, overlay radial, antrean konversi)
const fs = require('fs');
const path = require('path');
const { execFile, spawn } = require('child_process');
const { app, BrowserWindow, Tray, Menu, Notification, nativeImage, screen, ipcMain, shell, powerMonitor, clipboard, dialog } = require('electron');
const { uIOhook, UiohookKey } = require('uiohook-napi');
const { convert, uniquePath, CATEGORIES, FALLBACK_TARGETS } = require('./converter');
const { TOOLS, toolTasks } = require('./tools');

const APP_ID = 'com.local.shiftwheel'; // harus sama dengan build.appId di package.json
const OVERLAY_W = 360;
const OVERLAY_H = 380;
const HUD_W = 340;
const HUD_H = 92;
const HUD_DELAY = 600; // ms sebelum HUD progres muncul
const WHEEL_CENTER = 180; // pusat roda di dalam jendela overlay
const DRAG_THRESHOLD = 16; // DIP (~20 px fisik di skala 125%)
const HIDE_DELAY = 400; // beri waktu event drop sampai sebelum overlay hilang
const LINGER_MS = 8000; // mode --convert: tunggu sebentar supaya notifikasi masih bisa diklik
const LOGIN_ARGS = ['--hidden'];

// Ketuk tombol saat drag untuk membuka roda; ketuk tombol lain untuk ganti roda
const TRIGGER = {
  [UiohookKey.Shift]: 'convert',
  [UiohookKey.ShiftRight]: 'convert',
  [UiohookKey.Ctrl]: 'tools',
  [UiohookKey.CtrlRight]: 'tools',
  [UiohookKey.Alt]: 'actions',
  [UiohookKey.AltRight]: 'actions',
};

// Hemat sumber daya: roda sederhana cukup dirender CPU (proses GPU ~80 MB -> ~15 MB, transparansi
// tetap jalan), dan network service digabung ke proses utama karena Shiftwheel tidak memakai jaringan.
app.disableHardwareAcceleration();
app.commandLine.appendSwitch('enable-features', 'NetworkServiceInProcess2');

// Saat dev (npm start) belum ada shortcut Start Menu dengan AUMID ini, jadi toast
// diarahkan ke electron.exe supaya tetap muncul. Versi terinstal memakai APP_ID.
app.setAppUserModelId(app.isPackaged ? APP_ID : process.execPath);

const hidden = process.argv.includes('--hidden');
let convertOnly = parseConvert(process.argv) !== null;
let enabled = true;
let soundOn = true; // dari settings.json, bisa diubah di menu tray
let tray = null;
let overlay = null;

// ---------- Argumen --convert <format> <path> ----------

function parseConvert(argv) {
  const i = argv.indexOf('--convert');
  return i === -1 ? null : { fmt: String(argv[i + 1] || '').toLowerCase(), file: argv[i + 2] };
}

function handleArgv(argv) {
  const job = parseConvert(argv);
  if (job) enqueue([job.file], job.fmt);
  return job !== null;
}

// ---------- Pengaturan (userData/settings.json) ----------

const settingsFile = () => path.join(app.getPath('userData'), 'settings.json');

function loadSettings() {
  try { return JSON.parse(fs.readFileSync(settingsFile(), 'utf8')); } catch { return {}; }
}

function saveSettings(settings) {
  fs.mkdirSync(path.dirname(settingsFile()), { recursive: true });
  fs.writeFileSync(settingsFile(), JSON.stringify(settings, null, 2));
}

// ---------- Autostart ----------

function isAutostart() {
  const s = app.getLoginItemSettings({ args: LOGIN_ARGS });
  // executableWillLaunchAtLogin ikut memperhitungkan "Disabled" di Task Manager
  return s.executableWillLaunchAtLogin ?? s.openAtLogin;
}

function setAutostart(on) {
  if (!app.isPackaged) return; // jangan daftarkan electron.exe ke startup saat dev
  app.setLoginItemSettings({ openAtLogin: on, name: APP_ID, args: LOGIN_ARGS });
}

// ---------- Notifikasi ----------

const liveNotes = new Set(); // simpan referensi supaya event klik tidak hilang karena GC

function notify(title, body, onClick) {
  if (!Notification.isSupported()) return;
  const n = new Notification({ title, body: body || '' });
  liveNotes.add(n);
  n.on('click', () => onClick && onClick());
  n.on('close', () => liveNotes.delete(n));
  n.show();
}

// ---------- Antrean (sequential) untuk konversi dan tools ----------

let queue = Promise.resolve();
let pending = 0;
let batch = newBatch();
let quitTimer = null;

function newBatch() {
  return { ok: 0, fail: 0, skip: 0, done: 0, total: 0, lastOut: null, note: null, label: null, started: 0 };
}

function updateProgress() {
  if (tray) tray.setToolTip(pending ? `Shiftwheel: memproses ${pending} file...` : 'Shiftwheel');
}

// task: { name, label, chip, run: (opts) => Promise<path hasil> }
function enqueueTasks(tasks) {
  clearTimeout(quitTimer);
  for (const task of tasks) {
    pending++;
    batch.total++;
    batch.started = batch.started || Date.now();
    batch.label = batch.label === null || batch.label === task.label ? task.label : 'Selesai diproses';
    queue = queue.then(async () => {
      updateProgress();
      const info = { name: task.name, label: task.label, chip: task.chip, index: batch.done + 1, total: batch.total, percent: null };
      // HUD baru muncul kalau tugas berjalan agak lama, supaya konversi cepat tidak berkedip
      if (hudVisible) hudUpdate(info);
      else hudTimer = setTimeout(() => hudUpdate(info), HUD_DELAY);
      const onProgress = (p) => { info.percent = p; if (hudVisible) hudUpdate(info); };
      try {
        batch.lastOut = await task.run({ onProgress });
        batch.ok++;
      } catch (err) {
        if (err.skipped) batch.skip++;
        else batch.fail++;
        batch.note = batch.note || `${task.name}: ${err.message}`;
      }
      clearTimeout(hudTimer);
      batch.done++;
      if (--pending === 0) {
        hudHide();
        finishBatch();
      }
      updateProgress();
    });
  }
}

function enqueue(files, fmt) {
  const label = `Konversi ke ${fmt.toUpperCase()}`;
  enqueueTasks(files.map((f) => ({
    name: path.basename(String(f || '')), label, chip: fmt.toUpperCase(), run: (opts) => convert(f, fmt, opts),
  })));
}

function finishBatch() {
  const { ok, fail, skip, total, lastOut, note, label, started } = batch;
  batch = newBatch();
  const open = lastOut ? () => shell.showItemInFolder(lastOut) : null;
  // Chime selesai hanya untuk proses lama; yang instan cukup bunyi "pilih" saat drop
  if (fail) playSound('error');
  else if (ok && Date.now() - started > 1500) playSound('done');
  if (!fail && !skip) {
    notify(ok === 1 ? `${path.basename(lastOut)} siap` : `${ok} file selesai`, `${label}. Klik untuk membuka foldernya.`, open);
  } else if (!ok && !fail) {
    notify(total === 1 ? 'Dilewati' : `${total} file dilewati`, note);
  } else if (!ok) {
    notify(total === 1 ? 'Gagal diproses' : `${total} file gagal diproses`, note);
  } else {
    notify(`${ok} dari ${total} file selesai`, `${fail + skip} tidak diproses. ${note}`, open);
  }

  // Dipanggil dari menu klik kanan tanpa instance lain: keluar setelah antrean kosong
  if (convertOnly) quitTimer = setTimeout(() => { if (pending === 0) app.quit(); }, LINGER_MS);
}

// ---------- HUD progres (pojok kanan bawah) ----------

let hud = null;
let hudVisible = false;
let hudTimer = null;
let hudLastSend = 0;

function floatingWindow(width, height) {
  const win = new BrowserWindow({
    width,
    height,
    show: false,
    frame: false,
    transparent: true,
    resizable: false,
    movable: false,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    skipTaskbar: true,
    focusable: false,
    hasShadow: false,
    alwaysOnTop: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      backgroundThrottling: false,
      spellcheck: false,
      autoplayPolicy: 'no-user-gesture-required', // suara overlay tanpa klik dulu
    },
  });
  win.setAlwaysOnTop(true, 'screen-saver');
  win.webContents.on('will-navigate', (e) => e.preventDefault());
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  return win;
}

function hudUpdate(info) {
  if (!hud) {
    hud = floatingWindow(HUD_W, HUD_H);
    hud.loadFile(path.join(__dirname, 'hud.html'));
    hud.webContents.once('did-finish-load', () => hudVisible && hud.webContents.send('hud:update', info));
  }
  if (!hudVisible) {
    const wa = screen.getDisplayNearestPoint(screen.getCursorScreenPoint()).workArea;
    hud.setBounds({ x: wa.x + wa.width - HUD_W - 8, y: wa.y + wa.height - HUD_H - 4, width: HUD_W, height: HUD_H });
    hud.showInactive();
    hudVisible = true;
  } else if (Date.now() - hudLastSend < 100 && info.percent !== 1) {
    return; // batasi update progres ~10x per detik
  }
  hudLastSend = Date.now();
  if (!hud.webContents.isLoading()) hud.webContents.send('hud:update', info);
}

function hudHide() {
  clearTimeout(hudTimer);
  if (!hudVisible) return;
  hudVisible = false;
  hud.webContents.send('hud:hide');
  // Hancurkan setelah animasi: renderer HUD tidak perlu menganggur di memori (dibuat lagi saat dibutuhkan)
  setTimeout(() => {
    if (hudVisible || !hud) return;
    hud.destroy();
    hud = null;
  }, 250);
}

// ---------- Aksi file (roda Alt) ----------

const count = (paths) => (paths.length === 1 ? `"${path.basename(paths[0])}"` : `${paths.length} item`);
const psQuote = (s) => `'${s.replace(/'/g, "''")}'`;

// Electron tidak bisa menaruh file (CF_HDROP) ke clipboard; PowerShell 5.1 bisa
function copyFilesToClipboard(paths) {
  const script = `Set-Clipboard -LiteralPath ${paths.map(psQuote).join(',')}`;
  const args = ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(script, 'utf16le').toString('base64')];
  return new Promise((resolve, reject) => {
    execFile('powershell.exe', args, { windowsHide: true }, (err) => (err ? reject(new Error('Gagal menyalin file ke clipboard.')) : resolve()));
  });
}

async function transfer(paths, move) {
  const { canceled, filePaths } = await dialog.showOpenDialog({
    title: move ? 'Pindahkan ke folder' : 'Salin ke folder',
    properties: ['openDirectory', 'createDirectory'],
  });
  if (canceled || !filePaths[0]) return null;
  const dir = filePaths[0];
  let last = null;
  for (const src of paths) {
    if (move && path.dirname(src) === dir) continue;
    const isFile = fs.statSync(src).isFile();
    const ext = isFile ? path.extname(src).slice(1) : '';
    const dest = uniquePath(dir, isFile ? path.basename(src, path.extname(src)) : path.basename(src), ext);
    if (move) {
      try {
        fs.renameSync(src, dest);
      } catch (err) {
        if (err.code !== 'EXDEV') throw err;
        fs.cpSync(src, dest, { recursive: true, errorOnExist: true, force: false }); // beda drive: salin lalu hapus
        fs.rmSync(src, { recursive: true });
      }
    } else {
      fs.cpSync(src, dest, { recursive: true, errorOnExist: true, force: false });
    }
    last = dest;
  }
  return { msg: `${count(paths)} ${move ? 'dipindah' : 'disalin'} ke ${dir}`, reveal: last };
}

function openTerminal(paths) {
  const p = paths[0];
  const dir = fs.statSync(p).isDirectory() ? p : path.dirname(p);
  const term = spawn('wt.exe', ['-d', dir], { detached: true, stdio: 'ignore' });
  term.on('error', () => spawn('cmd.exe', ['/K'], { cwd: dir, detached: true, stdio: 'ignore' }).unref()); // tanpa Windows Terminal
  term.unref();
}

const ACTIONS = [
  { id: 'copy-path', icon: 'link', label: 'Salin path', caption: 'Salin path lengkap sebagai teks', run: (p) => { clipboard.writeText(p.join('\r\n')); return { msg: `Path ${count(p)} disalin` }; } },
  { id: 'copy', icon: 'copy', label: 'Salin', caption: 'Salin ke clipboard, siap di-paste', run: async (p) => { await copyFilesToClipboard(p); return { msg: `${count(p)} siap di-paste` }; } },
  { id: 'copy-to', icon: 'copy-to', label: 'Salin ke', caption: 'Salin ke folder lain', run: (p) => transfer(p, false) },
  { id: 'move-to', icon: 'move-to', label: 'Pindah ke', caption: 'Pindahkan ke folder lain', run: (p) => transfer(p, true) },
  { id: 'terminal', icon: 'terminal', label: 'Terminal', caption: 'Buka foldernya di Terminal', run: openTerminal },
  { id: 'reveal', icon: 'folder', label: 'Explorer', caption: 'Tampilkan di Explorer', run: (p) => shell.showItemInFolder(p[0]) },
];

async function runAction(id, paths) {
  const action = ACTIONS.find((a) => a.id === id);
  if (!action) return;
  try {
    const existing = paths.filter((p) => fs.existsSync(p));
    if (!existing.length) throw new Error('File tidak ditemukan.');
    const result = await action.run(existing);
    if (result) notify(result.msg, result.reveal ? 'Klik untuk membuka foldernya.' : '', result.reveal ? () => shell.showItemInFolder(result.reveal) : null);
  } catch (err) {
    playSound('error');
    notify(`${action.label} gagal`, err.message);
  }
}

// ---------- Isi roda (dikirim ke overlay) ----------

function wheelConfig() {
  const conv = (t) => ({ id: `convert:${t}`, label: t.toUpperCase(), caption: `Konversi ke ${t.toUpperCase()}` });
  const convert = { fallback: FALLBACK_TARGETS.map(conv) };
  for (const [cat, c] of Object.entries(CATEGORIES)) convert[cat] = c.targets.map(conv);
  convert.image.find((i) => i.id === 'convert:txt').caption = 'Ambil teks dari gambar (OCR)';
  const tools = {};
  for (const [cat, list] of Object.entries(TOOLS)) {
    tools[cat] = list.map((t) => ({ id: `tool:${cat}.${t.id}`, label: t.label, caption: t.caption, icon: t.icon, multi: Boolean(t.multi) }));
  }
  const actions = ACTIONS.map((a) => ({ id: `action:${a.id}`, label: a.label, caption: a.caption, icon: a.icon }));
  return { convert, tools, actions };
}

ipcMain.on('overlay:drop', (e, msg) => {
  if (!overlay || e.sender !== overlay.webContents) return;
  const id = msg && typeof msg.id === 'string' ? msg.id : '';
  const paths = Array.isArray(msg && msg.paths) ? msg.paths.filter((p) => typeof p === 'string' && p) : [];
  if (!id || paths.length === 0) return; // drop di tengah / luar segmen = batal

  const sep = id.indexOf(':');
  const kind = id.slice(0, sep);
  const value = id.slice(sep + 1);
  if (kind === 'convert') enqueue(paths, value);
  else if (kind === 'action') runAction(value, paths);
  else if (kind === 'tool') {
    try { enqueueTasks(toolTasks(value, paths)); } catch (err) { notify('Tool tidak bisa dipakai', err.message); }
  }
});

// ---------- Overlay ----------

let shown = false;
let hideTimer = null;

function createOverlay() {
  overlay = floatingWindow(OVERLAY_W, OVERLAY_H);
  overlay.loadFile(path.join(__dirname, 'overlay.html'));
  overlay.webContents.on('did-finish-load', () => {
    overlay.webContents.send('overlay:config', wheelConfig());
    overlay.webContents.send('overlay:settings', { sound: soundOn });
  });
  overlay.on('session-end', stopHook); // logoff / shutdown Windows
}

function placeOverlay(p) {
  const wa = screen.getDisplayNearestPoint(p).workArea;
  const clamp = (v, min, max) => Math.min(Math.max(v, min), max);
  const x = clamp(Math.round(p.x - WHEEL_CENTER), wa.x, wa.x + wa.width - OVERLAY_W);
  const y = clamp(Math.round(p.y - WHEEL_CENTER), wa.y, wa.y + wa.height - OVERLAY_H);
  overlay.setBounds({ x, y, width: OVERLAY_W, height: OVERLAY_H });
}

function openWheel(mode, cursor) {
  const fresh = !shown;
  clearTimeout(hideTimer);
  if (fresh) placeOverlay(cursor);
  overlay.webContents.send('overlay:show', { mode, fresh });
  if (fresh) overlay.showInactive();
  shown = true;
}

function scheduleHide() {
  if (!shown) return;
  shown = false;
  clearTimeout(hideTimer);
  overlay.webContents.send('overlay:close'); // animasi pudar
  hideTimer = setTimeout(() => overlay && overlay.hide(), HIDE_DELAY);
}

// Suara dimainkan di jendela overlay (main process tidak punya audio)
function playSound(name) {
  if (soundOn && overlay) overlay.webContents.send('overlay:sound', name);
}

function hideNow() {
  shown = false;
  clearTimeout(hideTimer);
  if (overlay) overlay.hide();
}

// ---------- Hook global (uiohook) ----------
// Heuristik "file sedang di-drag": tombol kiri ditahan dan kursor sudah bergeser > DRAG_THRESHOLD
// saat Shift/Ctrl/Alt diketuk. Roda tetap terbuka sampai mouse dilepas (atau Esc).
// Sengaja tanpa listener mousemove: jarak dihitung sekali saat tombol diketuk, jadi gerakan
// mouse sehari-hari hampir tidak memakan CPU.

const drag = { down: false, x0: 0, y0: 0 }; // posisi klik dalam DIP
const held = new Set(); // abaikan auto-repeat saat tombol ditahan
let hookRunning = false;

uIOhook.on('keydown', (e) => {
  if (e.keycode === UiohookKey.Escape) {
    if (shown) playSound('cancel');
    return hideNow();
  }
  const mode = TRIGGER[e.keycode];
  if (!mode || held.has(e.keycode)) return;
  held.add(e.keycode);
  if (!enabled || !drag.down) return;
  if (shown) return openWheel(mode); // ganti roda
  const cursor = screen.getCursorScreenPoint();
  if (Math.hypot(cursor.x - drag.x0, cursor.y - drag.y0) > DRAG_THRESHOLD) openWheel(mode, cursor);
});

uIOhook.on('keyup', (e) => held.delete(e.keycode));

uIOhook.on('mousedown', (e) => {
  if (e.button !== 1) return;
  // uiohook memberi piksel fisik; ubah ke DIP supaya benar di scaling selain 100%
  const p = screen.screenToDipPoint({ x: e.x, y: e.y });
  Object.assign(drag, { down: true, x0: p.x, y0: p.y });
});

uIOhook.on('mouseup', (e) => {
  if (e.button !== 1) return;
  drag.down = false;
  scheduleHide();
});

function startHook() {
  if (hookRunning || !enabled) return;
  try {
    uIOhook.start();
    hookRunning = true;
  } catch (err) {
    notify('Shiftwheel tidak bisa membaca keyboard/mouse', err.message);
  }
}

function stopHook() {
  if (!hookRunning) return;
  try { uIOhook.stop(); } catch { /* sudah berhenti */ }
  hookRunning = false;
  held.clear();
  drag.down = false;
  hideNow();
}

// ---------- Tray ----------

function buildMenu() {
  return Menu.buildFromTemplate([
    {
      label: 'Aktif',
      type: 'checkbox',
      checked: enabled,
      click: (item) => {
        enabled = item.checked;
        enabled ? startHook() : stopHook();
      },
    },
    {
      label: 'Suara',
      type: 'checkbox',
      checked: soundOn,
      click: (item) => {
        soundOn = item.checked;
        saveSettings({ ...loadSettings(), sound: soundOn });
        if (overlay) overlay.webContents.send('overlay:settings', { sound: soundOn });
      },
    },
    {
      label: app.isPackaged ? 'Jalankan saat Windows menyala' : 'Jalankan saat Windows menyala (hanya versi terinstal)',
      type: 'checkbox',
      checked: app.isPackaged && isAutostart(),
      enabled: app.isPackaged, // nonaktif saat dev
      click: (item) => setAutostart(item.checked),
    },
    { type: 'separator' },
    { label: `Shiftwheel ${app.getVersion()}${app.isPackaged ? '' : ' (dev)'}`, enabled: false },
    { label: 'Keluar', click: () => app.quit() },
  ]);
}

function trayImage() {
  // tray-16/24/32.png digambar khusus per ukuran oleh scripts/make-ico.js (lebih tajam dari hasil resize)
  const img = nativeImage.createEmpty();
  for (const [scaleFactor, size] of [[1, 16], [1.5, 24], [2, 32]]) {
    img.addRepresentation({ scaleFactor, buffer: fs.readFileSync(path.join(__dirname, 'assets', `tray-${size}.png`)) });
  }
  return img;
}

function createTray() {
  tray = new Tray(trayImage());
  updateProgress();
  // Menu dibangun ulang tiap dibuka supaya status autostart selalu sesuai registry
  const open = () => tray.popUpContextMenu(buildMenu());
  tray.on('click', open);
  tray.on('right-click', open);
}

// ---------- Mode latar belakang ----------

function startBackground() {
  convertOnly = false;
  clearTimeout(quitTimer);

  const settings = loadSettings();
  const firstRun = settings.firstRun !== false;
  soundOn = settings.sound !== false;
  if (firstRun) {
    setAutostart(true);
    saveSettings({ ...settings, firstRun: false });
  }

  createOverlay();
  createTray();
  startHook();

  powerMonitor.on('suspend', stopHook);
  powerMonitor.on('resume', () => { stopHook(); startHook(); }); // hook bisa mati setelah sleep

  if (firstRun && !hidden) notify('Shiftwheel siap', 'Mulai drag file, lalu ketuk Shift untuk konversi, Ctrl untuk tools, atau Alt untuk aksi.');
}

// ---------- Single instance ----------

if (!app.requestSingleInstanceLock({ argv: process.argv })) {
  app.quit();
} else {
  // Pakai additionalData: argv bawaan event ini bisa berubah urutan di Windows
  app.on('second-instance', (_e, _argv, _cwd, data) => {
    const argv = (data && data.argv) || _argv;
    if (handleArgv(argv)) return;
    if (convertOnly) return startBackground(); // app dibuka normal saat instance --convert masih jalan
    if (!argv.includes('--hidden')) notify('Shiftwheel sudah berjalan', 'Ikonnya ada di tray. Mulai drag file, lalu ketuk Shift.');
  });

  app.whenReady().then(() => {
    if (convertOnly) handleArgv(process.argv);
    else startBackground();
  });

  app.on('will-quit', stopHook);
}
