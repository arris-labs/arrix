// The probe's page (probe.rs). It reports, as JSON POSTed to /report:
// whether the page is cross-origin isolated; the cost of one Atomics.load,
// and of a cut that reads the counter every step, every Nth step and never;
// the time between Arris's steps; and the latency from a bump of the
// counter mid-cut to the cut's return, per reading interval, split into
// the wait for the poll that hears it and the unwinding after.
import init, { bump } from "./web_probe.js";

const params = new URLSearchParams(location.search);
const SIDE = Number(params.get("side") ?? 8);
const NTH = Number(params.get("nth") ?? 64);
const RUNS = Number(params.get("runs") ?? 3);
const LOADS = 1_000_000;
const SAMPLES = Number(params.get("samples") ?? 9);
const DELAY_MS = 50;

const clock = () => performance.timeOrigin + performance.now();
const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const send = (report) => fetch("/report", { method: "POST", body: JSON.stringify(report) });

// One request to the worker; `onStart` runs when the cut has begun, and
// the answer waits for it, so a late bump never lands in the next cut.
function call(worker, request, onStart) {
  return new Promise((resolve) => {
    let started = Promise.resolve();
    worker.onmessage = ({ data }) => {
      if (data.started) started = Promise.resolve(onStart?.());
      else started.then(() => resolve(data));
    };
    worker.postMessage(request);
  });
}

async function probe() {
  await init();
  const report = {
    isolated: self.crossOriginIsolated === true,
    shared: typeof SharedArrayBuffer === "function",
    side: SIDE,
    nth: NTH,
  };
  if (!report.shared) return report;
  const buffer = new SharedArrayBuffer(4);
  const counter = new Int32Array(buffer);
  const worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
  await call(worker, { op: "share", buffer });

  // The cost: one load, then the cut run to its end by reading interval,
  // then the time between steps (a timed run, whose clock reads cost too).
  const loads = await call(worker, { op: "loads", n: LOADS });
  report.load_ns = (loads.ms * 1e6) / LOADS;
  report.cost = {};
  for (const [name, every] of [["never", 0], ["every", 1], ["nth", NTH]]) {
    const runs = [];
    for (let i = 0; i < RUNS; i++) runs.push(await call(worker, { op: "cut", side: SIDE, every, timed: false }));
    report.cost[name] = { ms: median(runs.map((r) => r.ms)), steps: runs[0].steps, stopped: runs.some((r) => r.stopped) };
  }
  const timed = await call(worker, { op: "cut", side: SIDE, every: 1, timed: true });
  report.steps = { gap_ms: timed.gap_ms, max_gap_ms: timed.max_gap_ms };

  // The latency: a bump DELAY_MS into the cut, to the cut's return.
  report.latency = {};
  for (const [name, every] of [["every", 1], ["nth", NTH]]) {
    const samples = [];
    for (let i = 0; i < SAMPLES; i++) {
      let bumped = null;
      const r = await call(worker, { op: "cut", side: SIDE, every, timed: true }, async () => {
        await sleep(DELAY_MS);
        bumped = clock();
        bump(counter);
      });
      const heard = r.heard ?? r.returned;
      samples.push({
        stopped: r.stopped,
        ms: bumped === null ? null : r.returned - bumped,
        wait_ms: bumped === null ? null : heard - bumped,
        unwind_ms: r.returned - heard,
        // The poll that heard the bump is the first one after it.
        first_poll_after: bumped !== null && r.heard !== null && r.polled !== null && r.polled < bumped && bumped <= r.heard,
        steps: r.steps,
      });
    }
    const field = (k) => samples.map((s) => s[k]);
    report.latency[name] = {
      median_ms: median(field("ms")),
      max_ms: Math.max(...field("ms")),
      median_wait_ms: median(field("wait_ms")),
      median_unwind_ms: median(field("unwind_ms")),
      all_stopped: samples.every((s) => s.stopped && s.ms !== null),
      all_first_poll_after: samples.every((s) => s.first_poll_after),
      samples,
    };
  }
  worker.terminate();
  return report;
}

probe().then(send, (e) => send({ error: String(e?.stack ?? e) }));
