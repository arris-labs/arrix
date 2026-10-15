# Resources: half the cores, and a memory budget

Heavy runs share the machine with other work (Arris's tests run in another
terminal). systemd-oomd kills the whole terminal scope (tmux, the agent, the
build) when the session's memory *pressure* stays above its limit, and the
compile, link and lavapipe phases take gigabytes per parallel job; a CPU cap
does not prevent it.

- **Every heavy command goes through `scripts/heavy`**: a full or workspace
  `cargo build`/`test`/`clippy`, `arrix-app`'s tests, the gate (pre-commit
  and `scripts/gate` re-exec themselves under it). It runs the command in
  its own scope with `MemoryHigh`/`MemoryMax` and `nice -n 10`, so pressure
  kills the run and spares the terminal. `HEAVY_HIGH` and `HEAVY_MAX`
  override its limits (default 40% and 55% of RAM); its printed peak counts
  page cache, so the oomd journal is the real signal.
- **One gate at a time per target directory.** `scripts/gate` holds a
  `flock` on `target/.gate.lock`, so a second run waits: the tests' scratch
  directories have fixed names, and two gates double the memory. A worktree
  has its own target and runs beside.
- **Budget the two projects together**: the gate runs **4 test threads**
  (`.config/nextest.toml`; nextest ignores `RUST_TEST_THREADS`),
  **`RAYON_NUM_THREADS=8`** and **`--jobs 8`** while Arris's tests may be
  running, never more than 16. The suite does not scale past 4 threads and
  a timing test flakes at 8 (ADR-0040 has the measurements).
- A cold or wide compile (first build, toolchain or dependency bump) is the
  heaviest phase: give it a lower `--jobs` and run it apart from the other
  terminal's.
- Run long builds and suites in the background.
- After an oomd kill, read `journalctl -u systemd-oomd` and rerun with fewer
  jobs. The machine side (oomd's limit, swap) is the human's,
  `docs/notes/oomd-kills.md`.
