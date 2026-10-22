// tests/web_app.rs: starts the web app on an empty start (no document),
// waits until it has drawn a frame, and POSTs to /report whether the page
// is isolated, the backend wgpu drew through, and debug_state() as JSON.
import { start } from "./main.js";

const TIMEOUT_MS = 60_000;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const send = (report) => fetch("/report", { method: "POST", body: JSON.stringify(report) });

async function run() {
  const handle = await start(document.getElementById("arrix"));
  const end = performance.now() + TIMEOUT_MS;
  for (;;) {
    const panic = handle.panic_message();
    if (panic) throw new Error(`the app panicked: ${panic}`);
    const state = handle.debug_state();
    if (state && JSON.parse(state).window) {
      return {
        isolated: self.crossOriginIsolated === true,
        webgpu: "gpu" in navigator,
        backend: handle.backend() ?? null,
        debug_state: JSON.parse(state),
      };
    }
    if (performance.now() > end) throw new Error("the app drew no frame");
    await sleep(20);
  }
}

run().then(send, (e) => send({ error: String(e?.stack ?? e) }));
