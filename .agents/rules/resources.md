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
  memory. A bare `cargo test --workspace` in the terminal is the mistake
  this rule exists for.
- **Budget the two projects together**, not each at half. Measured on this
  machine (32 CPUs, 61 GiB; `cargo test --workspace`, build warm): 8 test
  threads took 434 s with a 27.5 GiB peak, 16 took 405 s with a 46 GiB peak
  (it hit the 45 GiB `MemoryHigh`), and 32 was killed by oomd inside a
  minute, alone. The test phase alone summed to 429 s at 8 threads and
  293 s at 16 (1.5x for 1.7x the memory). So: **8 jobs and 8 test threads
  while Arris's tests may be running, 16 only when they are not, never
  more.** Set `--jobs`, `RUST_TEST_THREADS` and `RAYON_NUM_THREADS` to the
  same number, in `~/.cargo/config.toml` or by hand.
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
