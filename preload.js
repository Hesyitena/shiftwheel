// Jembatan overlay <-> main. Drop file ditangkap di sini supaya path asli bisa diambil.
const { contextBridge, ipcRenderer, webUtils } = require('electron');

contextBridge.exposeInMainWorld('shiftwheel', {
  onShow: (cb) => ipcRenderer.on('overlay:show', () => cb()),
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
  const seg = e.target.closest ? e.target.closest('[data-fmt]') : null;
  const paths = [...e.dataTransfer.files].map((f) => webUtils.getPathForFile(f)).filter(Boolean);
  // fmt null = drop di tengah / di luar segmen = batal
  ipcRenderer.send('overlay:drop', { fmt: seg ? seg.dataset.fmt : null, paths });
});
