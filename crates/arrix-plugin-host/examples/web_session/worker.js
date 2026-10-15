// The evaluator's worker (page.rs, ADR-0042): the same module as the page,
// its own instance. The messages that come while the module loads are kept
// and handed over in order.
import init, { evaluator } from "./web_session.js";

const early = [];
onmessage = ({ data }) => early.push(data);
await init();
evaluator(early);
