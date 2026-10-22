// The page's side of the web app (src/web.rs, ADR-0042): loads the module
// and starts the app on a canvas. The handle is also `window.arrix`, which
// a test page drives.
import init, { WebHandle } from "./arrix_app.js";

// The file input the app's Import command clicks (src/web_files.rs): what
// is picked goes to the app as bytes, an `.arrx` to open and a STEP file to
// import. Dropped files reach the app through eframe instead.
function fileInput(handle) {
  const input = document.createElement("input");
  input.type = "file";
  input.id = "arrix-open";
  input.accept = ".arrx,.step,.stp";
  input.multiple = true;
  input.hidden = true;
  input.addEventListener("change", async () => {
    for (const file of input.files) {
      handle.accept_file(file.name, new Uint8Array(await file.arrayBuffer()));
    }
    input.value = "";
  });
  document.body.append(input);
}

export async function start(canvas) {
  await init();
  const handle = new WebHandle();
  await handle.start(canvas, new URL("./worker.js", import.meta.url).href);
  fileInput(handle);
  window.arrix = handle;
  return handle;
}
