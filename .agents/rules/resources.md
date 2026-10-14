# Resources: half the cores

Heavy runs share the machine with other work (Arris's tests run in another
terminal), and a run that takes every CPU can kill a terminal.

- Use at most half of the available cores (`nproc / 2`) for anything
  parallel: cargo's `--jobs` and `[build] jobs`, `RUST_TEST_THREADS`,
  `RAYON_NUM_THREADS`, `arrix eval --jobs`, and the `-j` of any other tool.
- The machine's `~/.cargo/config.toml` sets this for cargo and tests; check
  it is there before a long build, and pass the flags by hand where it is not.
- Run long builds and test suites with `nice -n 10`, in the background.
