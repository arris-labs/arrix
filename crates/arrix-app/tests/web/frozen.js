// tests/web_app.rs: opens north-star-missing.arrx (the north star with a
// plugin the page lacks, its stored result kept), standoff.arrx (nothing
// stored) and north-star.arrx (every plugin Tier 0). Once a document has
// settled it POSTs what debug_state() said of its tree, its badges and its
// bodies, and for the first the form of the frozen feature.
import { start } from "./main.js";

const TIMEOUT_MS = 120_000;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const send = (report) => fetch("/report", { method: "POST", body: JSON.stringify(report) });

function state(handle) {
  const panic = handle.panic_message();
  if (panic) throw new Error(`the app panicked: ${panic}`);
  return JSON.parse(handle.debug_state());
}

// Waits until `name` is the active tab and has settled.
async function opened(handle, name, bytes) {
  handle.open_bytes(name, bytes);
  const end = performance.now() + TIMEOUT_MS;
  for (;;) {
    const s = state(handle);
    const tab = s.shell?.documents.tabs.find((t) => t.active);
    if (tab?.title === name && s.evaluation?.settled) return s;
    if (performance.now() > end) throw new Error(`${name} did not settle`);
    await sleep(20);
  }
}

const features = (s) => s.shell.tree.parts.flatMap((p) => p.features);

async function bytesOf(path) {
  return new Uint8Array(await (await fetch(path)).arrayBuffer());
}

async function run() {
  const handle = await start(document.getElementById("arrix"));

  const missing = await opened(handle, "missing", await bytesOf("north-star-missing.arrx"));
  const frozen = features(missing).filter((f) => f.state.startsWith("frozen"));
  handle.activate_feature(frozen[0].id);
  await sleep(100);
  const form = state(handle).shell.form;

  const standoff = await opened(handle, "standoff", await bytesOf("standoff.arrx"));
  const northStar = await opened(handle, "north-star", await bytesOf("north-star.arrx"));

  return {
    missing: {
      frozen: frozen.map((f) => f.state),
      badges: missing.badges ?? [],
      bodies: missing.bodies.length,
      readOnly: form?.read_only ?? null,
    },
    standoff: {
      badges: standoff.badges ?? [],
      notes: features(standoff).flatMap((f) => (f.note ? [f.note] : [])),
    },
    northStar: {
      frozen: features(northStar).filter((f) => f.state.startsWith("frozen")),
      badges: northStar.badges ?? [],
      bodies: northStar.bodies.length,
    },
  };
}

run().then(send, (e) => send({ error: String(e?.stack ?? e) }));
