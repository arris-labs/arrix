// The probe's worker (probe.rs): the same module as the page, its own
// instance. Each message is a request; each answer carries its times on
// the clock the page shares (timeOrigin + now).
import init, { share, prepare, cut, loads } from "./web_probe.js";

const ready = init();
const shared = (t) => performance.timeOrigin + t;

onmessage = async ({ data }) => {
  await ready;
  if (data.op === "share") {
    share(new Int32Array(data.buffer));
    postMessage({});
  } else if (data.op === "loads") {
    const start = performance.now();
    loads(data.n);
    postMessage({ ms: performance.now() - start });
  } else if (data.op === "cut") {
    prepare(data.side);
    postMessage({ started: true });
    const start = performance.now();
    const outcome = JSON.parse(cut(data.every, data.timed));
    const end = performance.now();
    const at = (t) => (t == null ? null : shared(t));
    postMessage({ ...outcome, ms: end - start, heard: at(outcome.heard), polled: at(outcome.polled), returned: shared(end) });
  }
};
