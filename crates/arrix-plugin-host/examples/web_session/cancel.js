// Cancellation in the browser (plans/c4 step 8, ADR-0042). Opens
// bracket.arrx and POSTs to /report what it measured:
//   stops    from a width edit submitted while the one before is inside the
//            gear's join, to the first event of the newer generation (the
//            native C1 benchmark's measurement), per run;
//   commit   a preview posted during a commit leaves the commit alone;
//   preview  a commit posted during a preview stops it.
import init, { Page } from "./web_session.js";

const TIMEOUT_MS = 90_000;
const RUNS = Number(new URL(import.meta.url).searchParams.get("runs") ?? 11);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const send = (report) => fetch("/report", { method: "POST", body: JSON.stringify(report) });

// A yield that does not wait out a timer's clamp, so the clock below reads
// an event within a millisecond of its arrival.
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

let k = 0;
/** A width never equal to the one before, in the range the sweep evaluates clean. */
const width = () => `${60 + 0.1 * ((k++ % 40) + 1)} mm`;

async function stops(page) {
  const runs = [];
  for (let i = 0; i < RUNS; i++) {
    const g = page.set_param("w", width());
    await until(`generation ${g} to reach the gear`, () => page.evaluated(g, "Spur gear"));
    if (page.finished(g)) throw new Error(`generation ${g} finished before it was superseded`);
    const t = performance.now();
    const newer = page.set_param("w", width());
    await until(`generation ${newer} to start`, () => page.evaluated(newer, ""), true);
    const ms = performance.now() - t;
    const staleFinished = page.finished(g);
    await until(`generation ${newer} to finish`, () => page.done(newer));
    runs.push({ ms, staleFinished, newerFinished: page.finished(newer) });
    if (!page.interruptible()) break; // each run is as long as two evaluations
  }
  return runs;
}

/** A preview posted while a commit evaluates: the commit finishes, the preview follows. */
async function previewBehindCommit(page) {
  const g = page.set_param("w", width());
  await until(`generation ${g} to reach the gear`, () => page.evaluated(g, "Spur gear"));
  page.preview_param(1, "w", width());
  await until(`generation ${g} to finish`, () => page.done(g));
  await until("the preview", () => page.previewed(1) > 0);
  return { finished: page.finished(g), previewed: page.previewed(1) };
}

/** A commit posted while a preview evaluates. */
async function commitOverPreview(page) {
  page.preview_param(2, "w", width());
  await sleep(150); // inside the preview's evaluation, which takes seconds
  const t = performance.now();
  const g = page.set_param("w", width());
  await until(`generation ${g} to start`, () => page.evaluated(g, ""), true);
  const ms = performance.now() - t;
  await until(`generation ${g} to finish`, () => page.done(g));
  await sleep(500); // a preview that was not stopped would be heard by now
  return { ms, finished: page.finished(g), previewed: page.previewed(2) };
}

async function run() {
  await init();
  const bytes = new Uint8Array(await (await fetch("bracket.arrx")).arrayBuffer());
  const page = new Page(bytes, new URL("./worker.js", import.meta.url).href);
  await until("generation 0", () => page.done(0));
  const report = {
    isolated: self.crossOriginIsolated === true,
    interruptible: page.interruptible(),
  };
  report.stops = await stops(page);
  report.commit = await previewBehindCommit(page);
  report.preview = await commitOverPreview(page);
  return report;
}

run().then(send, (e) => send({ error: String(e?.stack ?? e) }));
