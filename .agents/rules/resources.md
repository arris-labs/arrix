# Resources: half the cores, and a memory budget

Heavy runs share the machine with other work (Arris's tests run in another
terminal), and a run that squeezes memory can kill a terminal: systemd-oomd
kills the whole terminal scope (tmux, the agent, the build) when the user
session's memory *pressure* (time stalled on reclaim, not RAM used) stays
above its limit. A CPU cap does not prevent it: the compile and link phase
and the visual tests (lavapipe) take gigabytes per parallel job.

- **Every heavy command goes through `scripts/heavy`**: a full or workspace
  `cargo build`/`test`/`clippy`, `arrix-app`'s tests, the hooks (they
  re-exec themselves under it). It runs the command in a scope of its own
  with `MemoryHigh`/`MemoryMax` and `nice -n 10`, so what dies under
  pressure is the run, not the terminal, and it prints the scope's peak
  memory. A bare `cargo nextest run --workspace` in the terminal is the mistake
  this rule exists for.
- **One gate at a time per target directory.** The pre-push hook holds a
  `flock` on `target/.pre-push.lock`, so a second push waits for the first:
  the tests' scratch directories have fixed names and race, and two gates
  double the memory. A worktree has a target of its own and runs beside.
- **Budget the two projects together**, not each at half. Measured on this
  machine (32 CPUs, 61 GiB, build warm): `cargo test --workspace` summed to
  320 s at 2 threads, 280 s at 4, 293 s at 16, and 32 was killed by oomd
  inside a minute, alone; it runs one binary after another and does not
  scale. `cargo nextest run` (ADR-0040) took 132 to 137 s at 4 threads (three
  acceptance runs of the whole pre-push: 136, 137 and 175 s for the workspace
  run, the last beside 16 busy loops, with no timing failure) and 96 s at 8,
  where a timing test flaked. So the gate runs **4 test
  threads**, set in `.config/nextest.toml` (nextest ignores
  `RUST_TEST_THREADS`; `cargo test` alone still honours it), with
  **`RAYON_NUM_THREADS=8`** and **`--jobs 8`**
  for the compile while Arris's tests may be running, and never more than
  16. Set them in `~/.cargo/config.toml` or by hand. (`scripts/heavy`'s
  peak counts page cache, which fills `MemoryHigh`, so it says little about
  the memory a run needs; the oomd journal is the real signal.) Build time
  against `--jobs` is not measured.
- A cold or wide compile is the heaviest phase (a first build, a toolchain
  or dependency bump): pass a lower `--jobs` for it, and don't overlap it
  with the other terminal's.
- Run long builds and test suites in the background; `scripts/heavy`
  already applies `nice -n 10`. `HEAVY_HIGH` and `HEAVY_MAX` override its
  limits (default 40% and 55% of RAM).
- If oomd kills a run anyway, the journal says why
  (`journalctl -u systemd-oomd`); rerun with fewer jobs rather than
  repeating it. The machine side (oomd's limit and duration, swap) is the
  human's, noted in `docs/notes/oomd-kills.md`.
