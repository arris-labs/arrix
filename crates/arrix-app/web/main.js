// The page's side of the web app (src/web.rs, ADR-0042): loads the module
// and starts the app on a canvas. The handle is also `window.arrix`, which
// a test page drives.
import init, { WebHandle } from "./arrix_app.js";

export async function start(canvas) {
  await init();
  const handle = new WebHandle();
  await handle.start(canvas, new URL("./worker.js", import.meta.url).href);
  window.arrix = handle;
  return handle;
}
