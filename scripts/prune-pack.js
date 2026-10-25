// electron-builder afterPack hook: removes heavy unused GPU/shader compiler DLLs
// Shiftwheel runs with app.disableHardwareAcceleration(), so these are never loaded.
const fs = require('fs');
const path = require('path');

exports.default = async function (context) {
  const dir = context.appOutDir;
  const unused = ['dxcompiler.dll', 'vk_swiftshader.dll', 'd3dcompiler_47.dll'];
  for (const name of unused) {
    const file = path.join(dir, name);
    try {
      if (fs.existsSync(file)) {
        fs.rmSync(file);
        console.log(`Pruned unused file: ${name}`);
      }
    } catch {
      /* ignore if locked */
    }
  }
};
