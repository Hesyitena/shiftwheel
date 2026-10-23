// Bridge between the overlay/HUD and main. File drops are caught here so the real paths can be read.
const { contextBridge, ipcRenderer, webUtils } = require('electron');

const on = (channel) => (cb) => ipcRenderer.on(channel, (_e, payload) => cb(payload));

contextBridge.exposeInMainWorld('shiftwheel', {
  onConfig: on('overlay:config'),
  onSettings: on('overlay:settings'),
  onShow: on('overlay:show'),
  onClose: on('overlay:close'),
  onSound: on('overlay:sound'),
  onHud: on('hud:update'),
  onHudHide: on('hud:hide'),
});

// Always force 'copy': Shift while dragging means "move" in Explorer,
// and if the drop target answers with move, Explorer may delete the original file.
window.addEventListener('dragover', (e) => {
  e.preventDefault();
  e.dataTransfer.dropEffect = 'copy';
});

window.addEventListener('drop', (e) => {
  e.preventDefault();
  e.dataTransfer.dropEffect = 'copy';
  // The overlay writes the highlighted segment to body.dataset.pick (empty = cancel)
  const id = document.body.dataset.pick || null;
  const paths = [...e.dataTransfer.files].map((f) => webUtils.getPathForFile(f)).filter(Boolean);
  ipcRenderer.send('overlay:drop', { id, paths });
});
