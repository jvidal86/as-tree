//! End-to-end tests for the `as-tree` binary, run by `cargo test`.
//!
//! - `fixtures` replays every `test/fixture/*.txt` through the binary and
//!   compares stdout to the checked-in `.exp` golden file (this replaces the
//!   old Bazel fixture tests).
//! - The remaining tests pin specific CLI behavior with deterministic,
//!   piped input (no `find`, no filesystem coloring).

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Path to the binary under test, provided by Cargo for integration tests.
const BIN: &str = env!("CARGO_BIN_EXE_as-tree");

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

/// Run the binary with `args`, feeding `stdin`, and capture the result.
fn run(args: &[&str], stdin: &str) -> Run {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn as-tree");
    // The child may exit before reading stdin (e.g. an arg-validation error
    // exits immediately), which makes this write fail with BrokenPipe. That is
    // expected, so ignore the result; the temporary closes stdin (EOF) on drop.
    let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
    let out = child.wait_with_output().unwrap();
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

#[test]
fn fixtures() {
    let dir = manifest_dir().join("test/fixture");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("read test/fixture") {
        let input = entry.unwrap().path();
        if input.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue; // skip the .exp golden files
        }
        let exp = PathBuf::from(format!("{}.exp", input.display()));
        let expected = std::fs::read_to_string(&exp)
            .unwrap_or_else(|_| panic!("missing golden file {exp:?}"));
        // The fixture file is passed as the filename argument.
        let got = run(&[input.to_str().unwrap()], "").stdout;
        assert_eq!(got, expected, "fixture mismatch for {input:?}");
        checked += 1;
    }
    assert!(checked > 0, "no fixtures were found in {dir:?}");
}

#[test]
fn version_matches_crate() {
    let v = env!("CARGO_PKG_VERSION");
    assert_eq!(run(&["-v"], "").stdout.trim(), v);
    assert_eq!(run(&["--version"], "").stdout.trim(), v);
}

#[test]
fn help_lists_options() {
    let out = run(&["--help"], "");
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("Usage:"));
    assert!(out.stdout.contains("-L <level>"));
    assert!(out.stdout.contains("-f"));
}

#[test]
fn level_limits_depth() {
    let input = "a/b/c\na/b/d\n";
    assert_eq!(run(&["-L", "1", "--color", "never"], input).stdout, ".\n└── a\n");
    // At the truncation boundary the collapsed chain splits: `a` then `b`.
    assert_eq!(run(&["-L", "2", "--color", "never"], input).stdout, "a\n└── b\n");
}

#[test]
fn full_path_prefixes_entries() {
    let out = run(&["-f", "--color", "never"], "src/lib/a.rs\nsrc/lib/b.rs\n");
    assert!(out.stdout.contains("./src/lib/a.rs"), "got:\n{}", out.stdout);
    assert!(out.stdout.contains("./src/lib/b.rs"), "got:\n{}", out.stdout);
}

#[test]
fn color_never_emits_no_escape_bytes() {
    // Security-relevant: control bytes in a path must not reach stdout raw.
    let out = run(&["--color", "never"], "safe/\u{1b}[31mevil\u{1b}[0m/leaf\n");
    assert!(!out.stdout.contains('\u{1b}'), "escape byte leaked: {:?}", out.stdout);
}

#[test]
fn rejects_bad_level() {
    for bad in ["0", "abc", "-1"] {
        let out = run(&["-L", bad], "a/b\n");
        assert_eq!(out.code, 1, "-L {bad} should exit 1");
        assert!(out.stderr.contains("level"), "-L {bad} stderr: {}", out.stderr);
    }
    // Missing value.
    assert_eq!(run(&["-L"], "").code, 1);
}

#[test]
fn rejects_unknown_flag() {
    let out = run(&["--bogus"], "");
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("Unrecognized"));
}

#[test]
#[cfg(unix)]
fn broken_pipe_does_not_panic() {
    // Reproduce `as-tree | head`: a reader that closes the pipe after a few
    // bytes. Without the SIGPIPE reset, the next stdout write panics
    // ("failed printing to stdout: Broken pipe"). With it, as-tree exits quietly.
    // Big input so as-tree is still writing when we close the read end.
    let input: String = (0..200_000).map(|i| format!("path{i}\n")).collect();

    let mut child = Command::new(BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn as-tree");

    // Feed stdin from a thread (as-tree reads all input before printing).
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });

    // Read a little, then close our read end -> as-tree gets SIGPIPE on next write.
    let mut stdout = child.stdout.take().unwrap();
    let mut buf = [0u8; 64];
    let _ = stdout.read(&mut buf);
    drop(stdout);

    let status = child.wait().unwrap();
    let mut stderr = String::new();
    child.stderr.take().unwrap().read_to_string(&mut stderr).ok();
    let _ = writer.join();

    assert!(
        !stderr.contains("panicked"),
        "as-tree panicked on a broken pipe:\n{stderr}"
    );
    // 101 is Rust's unwind-panic exit code (test/debug build uses panic=unwind).
    assert_ne!(status.code(), Some(101), "as-tree panicked on a broken pipe");
}

#[test]
fn non_ascii_filename_arg_does_not_panic() {
    // Regression for the byte-slice arg panic: a multi-byte leading char must
    // not crash (exit 101). It should fail cleanly opening the missing file.
    let out = run(&["ñope-nonexistent.txt"], "");
    assert_ne!(out.code, 101, "panicked on non-ASCII arg: {}", out.stderr);
}
