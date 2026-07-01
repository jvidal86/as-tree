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
fn deeply_nested_single_line_does_not_crash() {
    // Regression: a single input line with many thousands of `/`-separated
    // components used to overflow the native call stack -- both while
    // printing (unbounded recursion in `_print`) and, independently, while
    // dropping the resulting `PathTrie` (the compiler-generated recursive
    // drop glue for the nested `BTreeMap<PathBuf, PathTrie>`). Confirmed
    // crashing (SIGABRT, exit 134) at ~15,000 components before the fix, in
    // both debug and release builds. 30,000 is comfortably past that
    // threshold and still fast (well under a second).
    let deep = std::iter::repeat("a").take(30_000).collect::<Vec<_>>().join("/");
    let out = run(&["--color", "never"], &format!("{deep}\n"));
    assert_ne!(out.code, 101, "panicked (debug unwind): {}", out.stderr);
    assert_ne!(out.code, 134, "aborted, likely stack overflow: {}", out.stderr);
    assert_eq!(out.code, 0, "expected success, got: {}", out.stderr);
}

#[test]
fn oversized_input_is_rejected_cleanly_not_oom() {
    // Regression / safety-net test for the MAX_TRIE_NODES cap: even after
    // fixing the stack overflow and O(depth^2) blowup above, a sufficiently
    // large *adversarial* input would still grow the process without limit
    // (bounded memory growth is not the same as *bounded memory*). Feed one
    // line with more distinct components than the real production limit
    // (2,000,000) and confirm the process fails promptly and cleanly --
    // not a crash/abort, not a hang, not multi-GB of memory -- rather than
    // being left to grow until the OS OOM-kills it (or something else on
    // the machine).
    let huge = std::iter::repeat("a").take(2_500_000).collect::<Vec<_>>().join("/");
    let out = run(&["--color", "never"], &format!("{huge}\n"));
    assert_ne!(out.code, 101, "panicked: {}", out.stderr);
    assert_ne!(out.code, 134, "aborted: {}", out.stderr);
    assert_eq!(out.code, 1, "expected a clean rejection, got: {}", out.stderr);
    assert!(
        out.stderr.contains("distinct path entries"),
        "expected the safety-limit message, got: {}",
        out.stderr
    );
}

#[test]
fn directory_as_filename_does_not_hang() {
    // Regression: a directory fd returns EISDIR on every read, never Ok(0)
    // (EOF). `input.lines().filter_map(Result::ok)` used to silently discard
    // that error and immediately re-poll, spinning forever at ~100% CPU with
    // no output. Verify it now fails promptly instead of hanging -- polled
    // with a hard deadline so a regression fails this test rather than
    // hanging `cargo test` itself.
    let mut child = Command::new(BIN)
        .arg(manifest_dir().join("src"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn as-tree");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            assert!(!status.success(), "expected a directory input to fail, not succeed");
            return;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("as-tree hung on a directory input (still running after 5s)");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
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
#[cfg(unix)]
fn invalid_utf8_arg_does_not_panic() {
    // Regression: std::env::args() panics on any argument that is not valid
    // Unicode, which is reachable on Unix since argv is raw bytes with no
    // encoding enforced by the kernel. Build a genuinely invalid-UTF-8
    // argument (a lone 0xFF, 0xFE pair -- not a valid UTF-8 sequence) and
    // confirm it no longer crashes the parser.
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let bad_arg = OsString::from_vec(vec![0xFF, 0xFE]);
    let mut child = Command::new(BIN)
        .arg(&bad_arg)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn as-tree");
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!stderr.contains("panicked"), "panicked on non-UTF-8 arg: {stderr}");
    assert_ne!(out.status.code(), Some(101), "panicked on non-UTF-8 arg: {stderr}");
}

#[test]
fn non_ascii_filename_arg_does_not_panic() {
    // Regression for the byte-slice arg panic: a multi-byte leading char must
    // not crash (exit 101). It should fail cleanly opening the missing file.
    let out = run(&["ñope-nonexistent.txt"], "");
    assert_ne!(out.code, 101, "panicked on non-ASCII arg: {}", out.stderr);
}
