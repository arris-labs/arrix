// The web session's page (page.rs). It opens bracket.arrx from bytes,
// waits for generation 0 to finish and mesh, sets the width to 70 mm, waits for
// that generation, and POSTs to /report what it heard of both, as JSON.
import init, { Page } from "./web_session.js";

const TIMEOUT_MS = 90_000;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const send = (report) => fetch("/report", { method: "POST", body: JSON.stringify(report) });

async function heard(page, generation) {
  const end = performance.now() + TIMEOUT_MS;
  while (!page.done(generation)) {
    if (performance.now() > end) throw new Error(`generation ${generation} did not finish and mesh`);
    await sleep(10);
  }
  return JSON.parse(page.report(generation));
}

async function run() {
  await init();
  const bytes = new Uint8Array(await (await fetch("bracket.arrx")).arrayBuffer());
  const page = new Page(bytes, new URL("./worker.js", import.meta.url).href);
  const opened = await heard(page, 0);
  const generation = page.set_param("w", "70 mm");
  const edited = await heard(page, generation);
  return {
    isolated: self.crossOriginIsolated === true,
    interruptible: page.interruptible(),
    generation,
    opened,
    edited,
  };
}

run().then(send, (e) => send({ error: String(e?.stack ?? e) }));
