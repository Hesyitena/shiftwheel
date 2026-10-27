(function () {
  if (!window.__TAURI__) return;
  const { listen } = window.__TAURI__.event;
  const { invoke } = window.__TAURI__.core;

  const on = (channel) => (cb) => {
    listen(channel, (e) => cb(e.payload));
  };

  window.shiftwheel = {
    // overlay
    getSettings: () => invoke('get_settings'),
    onSettings: on('overlay:settings'),
    onShow: on('overlay:show'),
    onClose: on('overlay:close'),
    onSound: on('overlay:sound'),
    revealed: (ok = true) => invoke('overlay_revealed', { ok: Boolean(ok) }),
    onDragEnter: on('overlay:drag_enter'),
    onDragOver: on('overlay:drag_over'),
    onDragDrop: on('overlay:drag_drop'),
    onDragLeave: on('overlay:drag_leave'),
    // progress card
    onHud: on('hud:update'),
    onHudHide: on('hud:hide'),
    hudReveal: () => invoke('hud_reveal'),
    // first run (shown in the progress-card window)
    getFirstRun: () => invoke('get_first_run'),
    onFirstRun: on('hud:first_run'),
    firstRunDone: (autostart) => invoke('first_run_done', { autostart: Boolean(autostart) }),
    // crop editor
    getCropRequest: () => invoke('get_crop_request'),
    onCropInit: on('crop:init'),
    cropDone: (rect) => invoke('crop_done', { rect }),
  };
})();
