// tests/c4_accept.rs: C4's acceptance in the web app (plans/c4, Acceptance).
// Opens bracket.arrx and POSTs to /report what it measured:
//   opened    the first generation's eval line, as the app wrote it;
//   stops     from a width edit made while the one before is inside the
//             gear's join, to the first event of the newer generation (the
//             native C1 benchmark's measurement), per run, the app hearing
//             the session's events when asked rather than at the next frame;
//   edited    the eval line after the width is set to 80 mm, and the saved
//             download (base64), which the test opens natively;
//   missing, standoff, northStar   the frozen documents of frozen.js.
import { start } from "./main.js";

const TIMEOUT_MS = 90_000;
const RUNS = 11;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const send = (report) => fetch("/report", { method: "POST", body: JSON.stringify(report) });

// A yield that does not wait out a timer's clamp, so the clock reads an
// event within a millisecond of its arrival.
const channel = new MessageChannel();
const tick = () =>
  new Promise((resolve) => {
    channel.port1.onmessage = () => resolve();
    channel.port2.postMessage(0);
  });

async function until(what, test, spin = false) {
  const end = performance.now() + TIMEOUT_MS;
  while (!test()) {
    if (performance.now() > end) throw new Error(`timed out waiting for ${what}`);
    await (spin ? tick() : sleep(5));
  }
}

function state(handle) {
  const panic = handle.panic_message();
  if (panic) throw new Error(`the app panicked: ${panic}`);
  return JSON.parse(handle.debug_state());
}

const features = (s) => s.shell.tree.parts.flatMap((p) => p.features);
const bytesOf = async (path) => new Uint8Array(await (await fetch(path)).arrayBuffer());
const width = (line) => line?.params.find((p) => p.name === "w")?.value;

// The line, as text, once the newest finished generation has width `w`
// (metres) and the document has settled.
async function heard(handle, w) {
  let text;
  await until(`a settled generation with w = ${w}`, () => {
    text = handle.eval_line() ?? "null";
    return width(JSON.parse(text)) === w && state(handle).evaluation?.settled;
  });
  return text;
}

let k = 0;
/** A width never equal to the one before, in the range the sweep evaluates clean. */
const nextWidth = () => `${60 + 0.1 * ((k++ % 40) + 1)} mm`;

async function stops(handle) {
  const runs = [];
  for (let i = 0; i < RUNS; i++) {
    handle.set_param("w", nextWidth());
    const g = handle.generation();
    await until(`generation ${g} to reach the gear`, () => handle.evaluated(g, "Spur gear"));
    if (handle.finished(g)) throw new Error(`generation ${g} finished before it was superseded`);
    const t = performance.now();
    handle.set_param("w", nextWidth());
    const newer = handle.generation();
    await until(`generation ${newer} to start`, () => handle.evaluated(newer, ""), true);
    const ms = performance.now() - t;
    const staleFinished = handle.finished(g);
    await until(`generation ${newer} to finish`, () => handle.finished(newer));
    runs.push({ ms, staleFinished, newerFinished: handle.finished(newer) });
  }
  return runs;
}

// The download of a save, caught by wrapping the anchor's click.
let saved;
const click = HTMLAnchorElement.prototype.click;
HTMLAnchorElement.prototype.click = function () {
  if (!this.download) return click.call(this);
  const name = this.download;
  saved = fetch(this.href)
    .then((r) => r.arrayBuffer())
    .then((b) => {
      let binary = "";
      for (const byte of new Uint8Array(b)) binary += String.fromCharCode(byte);
      return { name, base64: btoa(binary) };
    });
};

async function opened(handle, name, bytes) {
  handle.open_bytes(name, bytes);
  let s;
  await until(`${name} to settle`, () => {
    s = state(handle);
    return s.shell.documents.tabs.find((t) => t.active)?.title === name && s.evaluation?.settled;
  });
  return s;
}

async function run() {
  const handle = await start(document.getElementById("arrix"));
  const isolated = self.crossOriginIsolated === true;

  handle.open_bytes("tests/docs/bracket", await bytesOf("bracket.arrx"));
  const first = await heard(handle, 0.06);
  const interruptible = handle.interruptible();
  const runs = await stops(handle);

  handle.set_param("w", "80 mm");
  const edited = await heard(handle, 0.08);
  handle.save_download();
  const download = await saved;

  const missing = await opened(handle, "missing", await bytesOf("north-star-missing.arrx"));
  const frozen = features(missing).filter((f) => f.state.startsWith("frozen"));
  handle.activate_feature(frozen[0].id);
  await sleep(100);
  const form = state(handle).shell.form;
  const standoff = await opened(handle, "standoff", await bytesOf("standoff.arrx"));
  const northStar = await opened(handle, "north-star", await bytesOf("north-star.arrx"));

  return {
    isolated,
    interruptible,
    opened: first,
    stops: runs,
    edited,
    download,
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
