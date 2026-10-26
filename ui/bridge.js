(function () {
  if (!window.__TAURI__) return;
  const { listen } = window.__TAURI__.event;
  const { invoke } = window.__TAURI__.core;

  const on = (channel) => (cb) => {
    listen(channel, (e) => cb(e.payload));
  };

  window.shiftwheel = {
    onConfig: on('overlay:config'),
    onSettings: on('overlay:settings'),
    onShow: on('overlay:show'),
    onClose: on('overlay:close'),
    onSound: on('overlay:sound'),
    revealed: (ok = true) => invoke('overlay_revealed', { ok: Boolean(ok) }),
    onHud: on('hud:update'),
    onHudHide: on('hud:hide'),
    onCropInit: on('crop:init'),
    cropDone: (rect) => invoke('crop_done', { rect }),
  };

  // Drag and drop bridge
  if (window.__TAURI__.window) {
    const { getCurrentWindow } = window.__TAURI__.window;
    const currentWin = getCurrentWindow();
    currentWin.onDragDropEvent((event) => {
      const payload = event.payload;
      if (!payload) return;
      if (payload.type === 'over') {
        if (payload.position && typeof window.shiftwheelHandleDragOver === 'function') {
          const x = payload.position.x / window.devicePixelRatio;
          const y = payload.position.y / window.devicePixelRatio;
          window.shiftwheelHandleDragOver(x, y);
        }
      } else if (payload.type === 'drop') {
        const id = document.body.dataset.pick || null;
        const paths = payload.paths || [];
        invoke('overlay_drop', { msg: { id, paths } });
      }
    });
  }
})();
