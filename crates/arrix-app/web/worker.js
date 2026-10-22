// The evaluator's worker (src/web.rs, ADR-0042): the same module as the
// page, its own instance. The messages that come while the module loads
// are kept and handed over in order.
import init, { run_evaluator } from "./arrix_app.js";

const early = [];
onmessage = ({ data }) => early.push(data);
await init();
run_evaluator(early);
