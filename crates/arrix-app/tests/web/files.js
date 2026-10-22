// tests/web_app.rs: files in and out of the web app. The bracket comes in
// through the page's file input (a DataTransfer and a `change`) and again
// dropped on the canvas; the first download is a save, the second an
// export; a STEP file picked by the input is imported into the document.
// Downloads are caught by wrapping the anchor's click, and POSTed to
// /report as their names and sizes with the bytes' leading text.
import { start } from "./main.js";

const TIMEOUT_MS = 40_000;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const send = (report) => fetch("/report", { method: "POST", body: JSON.stringify(report) });

const downloads = [];
const click = HTMLAnchorElement.prototype.click;
HTMLAnchorElement.prototype.click = function () {
  if (this.download) {
    const name = this.download;
    const taken = fetch(this.href)
      .then((r) => r.arrayBuffer())
      .then((b) => {
        const bytes = new Uint8Array(b);
        downloads.push({
          name,
          size: bytes.length,
          head: new TextDecoder().decode(bytes.slice(0, 16)),
        });
      });
    downloads.pending = (downloads.pending ?? []).concat(taken);
    return;
  }
  return click.call(this);
};

async function until(what, ready) {
  const end = performance.now() + TIMEOUT_MS;
  for (;;) {
    const value = await ready();
    if (value) return value;
    if (performance.now() > end) {
      throw new Error(`timed out: ${what}; the status line says: ${state(window.arrix).shell.status.hint}; tree ${JSON.stringify(state(window.arrix).shell.tree)}`);
    }
    await sleep(20);
  }
}

function state(handle) {
  const panic = handle.panic_message();
  if (panic) throw new Error(`the app panicked: ${panic}`);
  return JSON.parse(handle.debug_state());
}

// Settled: the active document finished, its bodies meshed.
const settled = (handle, tabs) => until(`${tabs} settled tabs`, () => {
  const s = state(handle);
  return s.shell.documents.tabs.length === tabs && s.evaluation?.settled && s;
});

function file(name, bytes) {
  const data = new DataTransfer();
  data.items.add(new File([bytes], name));
  return data;
}

async function fetchBytes(path) {
  return new Uint8Array(await (await fetch(path)).arrayBuffer());
}

async function download(what, act) {
  const before = downloads.length;
  act();
  await until(what, async () => {
    await Promise.all(downloads.pending ?? []);
    return downloads.length > before;
  });
  return downloads[before];
}

async function run() {
  const handle = await start(document.getElementById("arrix"));
  const bracket = await fetchBytes("bracket.arrx");

  // Picked: the input's change.
  const input = document.getElementById("arrix-open");
  input.files = file("bracket.arrx", bracket).files;
  input.dispatchEvent(new Event("change"));
  const picked = await settled(handle, 1);

  // Dropped on the canvas, which eframe reads.
  const canvas = document.getElementById("arrix");
  canvas.dispatchEvent(
    new DragEvent("drop", { dataTransfer: file("dropped.arrx", bracket), bubbles: true, cancelable: true }),
  );
  const dropped = await settled(handle, 2);

  const saved = await download("the save", () => handle.save_download());
  const exported = await download("the export", () => handle.run_command("core.file.export"));

  // Imported: a STEP file picked by the input becomes parts of the document.
  const parts = () => state(handle).shell.tree.parts.length;
  const before = parts();
  input.files = file("screw.step", await fetchBytes("screw.step")).files;
  input.dispatchEvent(new Event("change"));
  await until("the imported parts", () => parts() > before);
  const imported = parts() - before;

  const refused = await (async () => {
    input.files = file("notes.txt", new Uint8Array([1])).files;
    input.dispatchEvent(new Event("change"));
    return until("the notice", () => {
      const hint = state(handle).shell.status.hint;
      return hint.includes("notes.txt") && hint;
    });
  })();

  return {
    picked: picked.shell.documents.tabs.map((t) => t.title),
    dropped: dropped.shell.documents.tabs.map((t) => t.title),
    saved,
    exported,
    imported,
    refused,
  };
}

run().then(send, (e) => send({ error: String(e?.stack ?? e) }));
