extern crate ansi_term;
extern crate atty;
extern crate lscolors;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use lscolors::{LsColors, Style};

pub mod options;

#[derive(Debug, Default)]
pub struct PathTrie {
    // We rely on the sorted iteration order
    trie: BTreeMap<PathBuf, PathTrie>,
}

fn ansi_style_for_path(lscolors: &LsColors, path: &Path) -> ansi_term::Style {
    lscolors
        .style_for_path(&path)
        .map(Style::to_ansi_term_style)
        .unwrap_or_default()
}

/// Neutralize control characters in a path before it is printed to the
/// terminal. Path strings come from untrusted input (e.g. `find . | as-tree`),
/// and on Unix a filename may contain any byte except `/` and NUL -- including
/// ESC. Printing those raw lets a crafted filename inject terminal escape
/// sequences (color/output spoofing, OSC title or clipboard writes, carriage
/// return line rewrites). Control characters are rendered as visible escapes
/// (e.g. ESC -> `\u{1b}`); printable text, including non-ASCII Unicode, is left
/// untouched.
fn escape_control(s: &str) -> String {
    if !s.chars().any(char::is_control) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() {
            out.extend(c.escape_default());
        } else {
            out.push(c);
        }
    }
    out
}

impl PathTrie {
    fn contains_singleton_dir(&self) -> bool {
        self.trie.len() == 1 && !self.trie.iter().next().unwrap().1.trie.is_empty()
    }

    // Insert a path, keeping at most `max_level` components from the top.
    // `None` inserts the whole path (unlimited depth).
    pub fn insert(&mut self, path: &Path, max_level: Option<usize>) {
        let mut cur = self;
        for (depth, comp) in path.iter().enumerate() {
            if matches!(max_level, Some(max) if depth >= max) {
                break;
            }
            cur = cur
                .trie
                .entry(PathBuf::from(comp))
                .or_insert_with(PathTrie::default);
        }
    }

    fn _print(
        &self,
        top: bool,
        prefix: &str,
        join_with_parent: bool,
        lscolors: &LsColors,
        parent_path: PathBuf,
        full_path: bool,
    ) {
        let normal_prefix = format!("{}│   ", prefix);
        let last_prefix = format!("{}    ", prefix);

        for (idx, (path, it)) in self.trie.iter().enumerate() {
            let current_path = parent_path.join(path);
            let style = ansi_style_for_path(&lscolors, &current_path);

            let contains_singleton_dir = it.contains_singleton_dir();

            let painted = match full_path {
                false => style.paint(escape_control(&path.to_string_lossy())),
                true => match contains_singleton_dir && !join_with_parent {
                    false => style.paint(escape_control(&current_path.to_string_lossy())),
                    true => style.paint(String::new()),
                },
            };

            // If this folder only contains a single dir, we skip printing it because it will be
            // picked up and printed on the next iteration. If this is a full path (even if it
            // contains more than one directory), we also want to skip printing, because the full
            // path will be printed all at once (see painted above), not part by part.
            // If this is a full path however the prefix must be printed at the very beginning.
            let should_print = (contains_singleton_dir && !join_with_parent)
                || !contains_singleton_dir
                || !full_path;

            let newline = if contains_singleton_dir { "" } else { "\n" };
            let is_last = idx == self.trie.len() - 1;

            let next_prefix = if join_with_parent {
                let joiner = if full_path || top || parent_path == PathBuf::from("/") {
                    ""
                } else {
                    "/"
                };
                if should_print {
                    print!("{}{}{}", style.paint(joiner), painted, newline);
                }
                prefix
            } else if !is_last {
                if should_print {
                    print!("{}├── {}{}", prefix, painted, newline);
                }
                &normal_prefix
            } else {
                if should_print {
                    print!("{}└── {}{}", prefix, painted, newline);
                }
                &last_prefix
            };

            it._print(
                false,
                next_prefix,
                contains_singleton_dir,
                lscolors,
                current_path,
                full_path,
            )
        }
    }

    fn print(&self, lscolors: &LsColors, full_path: bool) {
        if self.trie.is_empty() {
            println!();
            return;
        }

        // This works because PathBuf::from(".").join(PathBuf::from("/")) == PathBuf::from("/")
        let current_path = PathBuf::from(".");
        let contains_singleton_dir = self.contains_singleton_dir();

        if !contains_singleton_dir {
            let style = ansi_style_for_path(&lscolors, &current_path);
            println!("{}", style.paint(current_path.to_string_lossy()));
        }

        self._print(
            true,
            "",
            contains_singleton_dir,
            &lscolors,
            current_path,
            full_path,
        )
    }
}

fn drain_input_to_path_trie<T: BufRead>(input: &mut T, max_level: Option<usize>) -> PathTrie {
    let mut trie = PathTrie::default();

    for path_buf in input.lines().filter_map(Result::ok).map(PathBuf::from) {
        trie.insert(&path_buf, max_level)
    }

    trie
}

/// Restore the default `SIGPIPE` handler so as-tree behaves like a normal Unix
/// filter: when a downstream reader closes the pipe early (e.g. `as-tree | head`)
/// it exits quietly instead of panicking on the failed stdout write. Rust
/// installs `SIG_IGN` for `SIGPIPE` at startup and surfaces the broken pipe as an
/// `io::Error`, which `print!` turns into a panic.
#[cfg(unix)]
fn reset_sigpipe() {
    // Safety: resetting a signal to its default disposition is async-signal-safe,
    // and this runs at the very start of `main`, before any output or threads.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

fn main() -> io::Result<()> {
    reset_sigpipe();
    let options = options::parse_options_or_die();

    let trie = match &options.filename {
        None => {
            if atty::is(atty::Stream::Stdin) {
                eprintln!("Warning: reading from stdin, which is a tty.");
            }
            drain_input_to_path_trie(&mut io::stdin().lock(), options.max_level)
        }
        Some(filename) => {
            let file = File::open(filename)?;
            let mut reader = BufReader::new(file);
            drain_input_to_path_trie(&mut reader, options.max_level)
        }
    };

    let lscolors = match &options.colorize {
        options::Colorize::Always => LsColors::from_env().unwrap_or_default(),
        options::Colorize::Auto => {
            if atty::is(atty::Stream::Stdout) {
                LsColors::from_env().unwrap_or_default()
            } else {
                LsColors::empty()
            }
        }
        options::Colorize::Never => LsColors::empty(),
    };

    trie.print(&lscolors, options.full_path);

    io::Result::Ok(())
}

#[cfg(test)]
mod tests {
    use super::{escape_control, PathTrie};
    use std::path::PathBuf;

    #[test]
    fn leaves_printable_text_untouched() {
        assert_eq!(escape_control("normal.txt"), "normal.txt");
        // Non-ASCII printable characters must survive unchanged.
        assert_eq!(escape_control("café/ñoño/中文/🦀"), "café/ñoño/中文/🦀");
        // The lossy UTF-8 replacement character is printable, not control.
        assert_eq!(escape_control("a\u{fffd}b"), "a\u{fffd}b");
    }

    #[test]
    fn escapes_control_characters() {
        // No raw control byte may appear in the output.
        assert_eq!(escape_control("\u{1b}[31mred"), "\\u{1b}[31mred"); // ESC
        assert_eq!(escape_control("a\u{7}b"), "a\\u{7}b"); // BEL
        assert_eq!(escape_control("REAL\rFAKE"), "REAL\\rFAKE"); // CR
        assert_eq!(escape_control("x\u{7f}y"), "x\\u{7f}y"); // DEL
        assert!(!escape_control("\u{1b}]0;title\u{7}").contains('\u{1b}'));
    }

    // Length of the longest chain of nodes = number of path components kept.
    fn levels(trie: &PathTrie) -> usize {
        trie.trie.values().map(|c| 1 + levels(c)).max().unwrap_or(0)
    }

    #[test]
    fn insert_unlimited_keeps_all_components() {
        let mut trie = PathTrie::default();
        trie.insert(&PathBuf::from("a/b/c/d"), None);
        assert_eq!(levels(&trie), 4);
    }

    #[test]
    fn insert_truncates_to_max_level() {
        let mut trie = PathTrie::default();
        trie.insert(&PathBuf::from("a/b/c/d"), Some(2));
        assert_eq!(levels(&trie), 2);
    }

    #[test]
    fn max_level_larger_than_depth_is_unlimited() {
        let mut trie = PathTrie::default();
        trie.insert(&PathBuf::from("a/b/c/d"), Some(99));
        assert_eq!(levels(&trie), 4);
    }
}
