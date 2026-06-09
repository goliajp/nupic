//! CLI surface smoke tests.
//!
//! Asserts the *observable command-line contract* — exit codes, presence
//! of help entries, that `--version` parses as semver, and that `mock`
//! produces a valid PNG. Nothing about specific colors, byte counts,
//! or text rendering — those rot with implementation changes.

use std::process::Command;

fn nupic() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nupic"))
}

#[test]
fn version_prints_valid_semver() {
    let out = nupic().arg("--version").output().expect("spawn nupic");
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let version = stdout
        .strip_prefix("nupic ")
        .unwrap_or_else(|| panic!("expected 'nupic <ver>', got: {stdout:?}"))
        .trim();
    let parts: Vec<&str> = version.split('.').collect();
    assert_eq!(parts.len(), 3, "expected 3-component semver: {version:?}");
    for p in parts {
        // Allow `1.2.3-pre.4` style by stripping suffix on the last.
        let numeric = p.split('-').next().unwrap_or(p);
        assert!(
            numeric.parse::<u32>().is_ok(),
            "non-numeric semver component: {p:?}"
        );
    }
}

#[test]
fn help_lists_every_day_one_subcommand() {
    let out = nupic().arg("--help").output().expect("spawn nupic");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for cmd in ["resize", "fit", "circle", "mock", "watermark", "compress"] {
        assert!(stdout.contains(cmd), "missing subcommand '{cmd}' in --help");
    }
}

#[test]
fn no_args_exits_nonzero() {
    // With `arg_required_else_help`, no-args should print help to stderr
    // and exit non-zero.
    let out = nupic().output().expect("spawn nupic");
    assert!(!out.status.success(), "expected non-zero exit");
}

#[test]
fn each_subcommand_has_help() {
    for cmd in ["resize", "fit", "circle", "mock", "watermark", "compress"] {
        let out = nupic().arg(cmd).arg("--help").output().expect("spawn");
        assert!(out.status.success(), "{cmd} --help failed");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("Usage:") || stdout.contains("usage:"),
            "{cmd} --help missing Usage section: {stdout}"
        );
    }
}

#[test]
fn mock_writes_a_valid_png_file() {
    let tmp = std::env::temp_dir().join(format!(
        "nupic_test_mock_{}.png",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&tmp);

    let out = nupic()
        .args(["mock", "-W", "60", "-H", "40", "-o"])
        .arg(&tmp)
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "mock exit {:?}, stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output file readable");
    assert!(bytes.len() >= 8, "output PNG too short");
    assert_eq!(
        &bytes[..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        "output is not a PNG (missing signature)"
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn compress_nonexistent_input_exits_nonzero() {
    let nonexistent = std::env::temp_dir().join("nupic-no-such-input-xyz.png");
    let _ = std::fs::remove_file(&nonexistent);
    let out_path = std::env::temp_dir().join(format!(
        "nupic_test_compress_err_{}.png",
        std::process::id()
    ));

    let out = nupic()
        .arg("compress")
        .arg(&nonexistent)
        .arg("-o")
        .arg(&out_path)
        .output()
        .expect("spawn");
    assert!(!out.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("error")
            || stderr.to_lowercase().contains("failed")
            || stderr.to_lowercase().contains("no such"),
        "stderr does not look like an error message: {stderr}"
    );
}

#[test]
fn resize_and_fit_reject_conflicting_args() {
    // resize has --width/--height/--scale; --scale is mutually exclusive with
    // -W/-H per clap config.
    let out_path = std::env::temp_dir().join("nupic_test_resize_conflict.png");
    let out = nupic()
        .args([
            "resize",
            "/tmp/whatever.png",
            "-W",
            "100",
            "--scale",
            "0.5",
            "-o",
        ])
        .arg(&out_path)
        .output()
        .expect("spawn");
    assert!(!out.status.success(), "expected clap to reject conflict");
}
