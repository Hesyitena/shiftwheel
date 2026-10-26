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
    onDragEnter: on('overlay:drag_enter'),
    onDragOver: on('overlay:drag_over'),
    onDragDrop: on('overlay:drag_drop'),
    onDragLeave: on('overlay:drag_leave'),
    drop: (id, paths) => invoke('overlay_drop', { msg: { id, paths } }),
  };
})();
