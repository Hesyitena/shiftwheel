// Jembatan overlay <-> main. Drop file ditangkap di sini supaya path asli bisa diambil.
const { contextBridge, ipcRenderer, webUtils } = require('electron');

contextBridge.exposeInMainWorld('shiftwheel', {
  onConfig: (cb) => ipcRenderer.on('overlay:config', (_e, config) => cb(config)),
  onShow: (cb) => ipcRenderer.on('overlay:show', (_e, payload) => cb(payload)),
});

// Wajib paksa 'copy': Shift saat drag berarti "move" di Explorer,
// dan kalau target membalas move, Explorer bisa menghapus file asli.
window.addEventListener('dragover', (e) => {
  e.preventDefault();
  e.dataTransfer.dropEffect = 'copy';
});

window.addEventListener('drop', (e) => {
  e.preventDefault();
  e.dataTransfer.dropEffect = 'copy';
  const seg = e.target.closest ? e.target.closest('[data-id]') : null;
  const paths = [...e.dataTransfer.files].map((f) => webUtils.getPathForFile(f)).filter(Boolean);
  // id null = drop di tengah / di luar segmen = batal
  ipcRenderer.send('overlay:drop', { id: seg ? seg.dataset.id : null, paths });
});
