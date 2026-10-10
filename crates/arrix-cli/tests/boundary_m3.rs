//! The open boundary's accept (plans/boundary-m3 step 8, ADR-0016): an
//! agent with only the installed `arrix` and `arrix-app` and the Python SDK
//! writes a plugin, loads it into a session, builds, saves, renders and
//! checks the render against its schema, tests the plugin with `arrix test`
//! and sees its document frozen without it. The agent is
//! `tests/boundary/agent.py`, run from a temporary directory holding copies
//! of the three, in an environment cleared of everything else.

mod common;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use common::{arrix_app, copy_tree, workspace};

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arrix-m3-accept-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// The directory of the `python3` on this test's PATH.
fn python_dir() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .find(|dir| dir.join("python3").is_file())
        .expect("python3 is on PATH")
}

/// Panics if `text` names the workspace.
fn assert_outside(what: &str, text: &OsStr, root: &Path) {
    let (text, root) = (text.to_string_lossy(), root.to_string_lossy());
    assert!(!text.contains(&*root), "{what} names the workspace: {text}");
}

#[test]
fn an_agent_with_only_the_binaries_and_the_sdk_does_the_whole_scenario() {
    let dir = scratch();
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_arrix"), bin.join("arrix")).unwrap();
    std::fs::copy(arrix_app(), bin.join("arrix-app")).unwrap();
    let sdk = dir.join("sdk");
    copy_tree(&workspace().join("python/arrix"), &sdk.join("arrix"));
    let _ = std::fs::remove_dir_all(sdk.join("arrix/__pycache__"));
    let agent = dir.join("agent.py");
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::copy(here.join("tests/boundary/agent.py"), &agent).unwrap();

    let path = std::env::join_paths([bin, python_dir()]).unwrap();
    let mut run = Process::new("python3");
    run.arg(&agent)
        .current_dir(&dir)
        .env_clear()
        .env("PATH", &path)
        .env("PYTHONPATH", &sdk);
    if let Some(gpu) = std::env::var_os("ARRIX_REQUIRE_GPU") {
        run.env("ARRIX_REQUIRE_GPU", gpu);
    }

    // The agent is given nothing of the workspace.
    let root = workspace().canonicalize().unwrap();
    for arg in run.get_args() {
        assert_outside("an argument", arg, &root);
    }
    for (name, value) in run.get_envs() {
        assert_outside(&name.to_string_lossy(), value.unwrap_or_default(), &root);
    }
    assert_outside("the working directory", dir.as_os_str(), &root);

    let out = run.output().expect("python3 runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the agent failed:\n{stdout}\n{stderr}"
    );
    for line in stdout.lines().filter(|l| l.starts_with("SKIPPED")) {
        eprintln!("{line} (boundary_m3)");
    }
    assert!(
        stdout.lines().any(|l| l == "ACCEPTED"),
        "{stdout}\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
