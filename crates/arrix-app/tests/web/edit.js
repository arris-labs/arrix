// tests/web_app.rs: opens bracket.arrx in the web app, waits for the
// generation it opens at to finish and settle, sets the width to 80 mm as
// one gesture through the handle, waits for that generation, and POSTs to
// /report what debug_state() and the eval line said of each, and the
// status line's hint just after opening.
import { start } from "./main.js";

const TIMEOUT_MS = 120_000;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const send = (report) => fetch("/report", { method: "POST", body: JSON.stringify(report) });

function width(line) {
  return line?.params.find((p) => p.name === "w")?.value;
}

// Waits until the newest finished line has width `w` (metres) and the
// document has settled (its bodies meshed): what was heard then.
async function heard(handle, w) {
  const end = performance.now() + TIMEOUT_MS;
  for (;;) {
    const panic = handle.panic_message();
    if (panic) throw new Error(`the app panicked: ${panic}`);
    // The line goes back as the text the app wrote: JSON.parse would make
    // 20.0 the integer 20, and the comparison is to the bit.
    const text = handle.eval_line() ?? "null";
    const state = JSON.parse(handle.debug_state());
    if (width(JSON.parse(text)) === w && state.evaluation?.settled) {
      return { line: text, evaluation: state.evaluation, bodies: state.bodies.length };
    }
    if (performance.now() > end) throw new Error(`no settled generation with w = ${w}`);
    await sleep(20);
  }
}

async function run() {
  const handle = await start(document.getElementById("arrix"));
  const bytes = new Uint8Array(await (await fetch("bracket.arrx")).arrayBuffer());
  handle.open_bytes("tests/docs/bracket", bytes);
  // A frame, so the status line has been drawn since the open.
  await sleep(100);
  const hint = JSON.parse(handle.debug_state()).shell.status.hint;
  const opened = await heard(handle, 0.06);
  const applied = handle.set_param("w", "80 mm");
  const edited = await heard(handle, 0.08);
  return {
    isolated: self.crossOriginIsolated === true,
    hint,
    applied,
    opened,
    edited,
  };
}

run().then(send, (e) => send({ error: String(e?.stack ?? e) }));
