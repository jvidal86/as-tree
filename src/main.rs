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

impl Drop for PathTrie {
    /// The compiler-generated drop glue for a nested `BTreeMap<PathBuf,
    /// PathTrie>` is recursive -- one native stack frame per level of
    /// nesting -- and can overflow the stack for a pathologically deep input
    /// (a single line with many thousands of `/`-separated components),
    /// independently of how `_print`/`insert` are implemented. Drain the
    /// tree iteratively instead: move each node's children onto a
    /// heap-allocated work list before that node is dropped, so by the time
    /// its own (recursive, compiler-generated) drop glue runs there is
    /// nothing left in it to recurse into.
    fn drop(&mut self) {
        let mut pending: Vec<PathTrie> = std::mem::take(&mut self.trie).into_values().collect();
        while let Some(mut node) = pending.pop() {
            pending.extend(std::mem::take(&mut node.trie).into_values());
        }
    }
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

    /// Depth-first pre-order walk that mirrors the old recursive `_print`, but
    /// uses an explicit heap-allocated `Vec` as the work stack instead of
    /// native call-stack recursion. A single input line with many
    /// `/`-separated components used to overflow the (fixed-size, ~8MB by
    /// default) native stack and abort the process; the `Vec`-based stack is
    /// bounded only by available heap memory.
    ///
    /// The line-drawing `prefix` and the accumulated `path_buf` are each grown
    /// and shrunk in place (push on descent, truncate/pop on return) instead
    /// of being rebuilt from scratch at every level. The old code allocated a
    /// fresh, full-length copy of both at every level, which is O(depth) work
    /// per level and therefore O(depth^2) total for a single deep chain;
    /// mutating in place makes each step O(1) amortized, so the whole
    /// traversal is O(depth) (well, O(number of nodes): the tie-breaker for a
    /// wide, shallow tree is still the number of entries, as it always was).
    ///
    /// `path_buf.push()` is used instead of `Path::join()` for descending, but
    /// they are equivalent here: both replace the whole accumulated path when
    /// the pushed component is itself absolute, which is exactly the behavior
    /// relied on to print a single collapsed line for an absolute input path
    /// all the way from `/`. Since a path component can only be absolute as
    /// the very first component of a line (see `PathTrie::insert`), that
    /// "replace" case can only trigger while `path_buf` is still empty, so it
    /// never conflicts with anything already pushed onto the shared buffer.
    fn print_tree(&self, top_join_with_parent: bool, lscolors: &LsColors, top_path: PathBuf, full_path: bool) {
        struct Frame<'t> {
            iter: std::collections::btree_map::Iter<'t, PathBuf, PathTrie>,
            len: usize,
            next_idx: usize,
            top: bool,
            join_with_parent: bool,
            // Length/depth `prefix`/`path_buf` should be restored to before
            // handling the next item yielded by this frame's iterator.
            prefix_len: usize,
            path_depth: usize,
        }

        let mut prefix = String::new();
        let mut path_buf = top_path;
        let mut path_buf_depth = 0usize;

        let mut stack: Vec<Frame> = vec![Frame {
            iter: self.trie.iter(),
            len: self.trie.len(),
            next_idx: 0,
            top: true,
            join_with_parent: top_join_with_parent,
            prefix_len: 0,
            path_depth: 0,
        }];

        while let Some(frame) = stack.last_mut() {
            prefix.truncate(frame.prefix_len);
            while path_buf_depth > frame.path_depth {
                path_buf.pop();
                path_buf_depth -= 1;
            }

            let Some((path, it)) = frame.iter.next() else {
                stack.pop();
                continue;
            };

            let is_last = frame.next_idx == frame.len - 1;
            frame.next_idx += 1;
            let top = frame.top;
            let join_with_parent = frame.join_with_parent;
            let base_prefix_len = frame.prefix_len;
            // `path_buf` still holds this frame's own path (nothing has been
            // pushed for the current item yet).
            let parent_is_root = path_buf.as_path() == Path::new("/");
            // `frame` (and its borrow of `stack`) is not used again below,
            // which lets us push a new frame onto `stack` further down.

            path_buf.push(path);
            path_buf_depth += 1;

            let style = ansi_style_for_path(lscolors, &path_buf);
            let contains_singleton_dir = it.contains_singleton_dir();

            let painted = match full_path {
                false => style.paint(escape_control(&path.to_string_lossy())),
                true => match contains_singleton_dir && !join_with_parent {
                    false => style.paint(escape_control(&path_buf.to_string_lossy())),
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

            let (child_prefix_len, child_join_with_parent) = if join_with_parent {
                let joiner = if full_path || top || parent_is_root { "" } else { "/" };
                if should_print {
                    print!("{}{}{}", style.paint(joiner), painted, newline);
                }
                (base_prefix_len, contains_singleton_dir)
            } else if !is_last {
                if should_print {
                    print!("{}├── {}{}", prefix, painted, newline);
                }
                prefix.push_str("│   ");
                (prefix.len(), contains_singleton_dir)
            } else {
                if should_print {
                    print!("{}└── {}{}", prefix, painted, newline);
                }
                prefix.push_str("    ");
                (prefix.len(), contains_singleton_dir)
            };

            stack.push(Frame {
                iter: it.trie.iter(),
                len: it.trie.len(),
                next_idx: 0,
                top: false,
                join_with_parent: child_join_with_parent,
                prefix_len: child_prefix_len,
                path_depth: path_buf_depth,
            });
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

        self.print_tree(contains_singleton_dir, lscolors, current_path, full_path)
    }
}

fn drain_input_to_path_trie<T: BufRead>(
    input: &mut T,
    max_level: Option<usize>,
) -> io::Result<PathTrie> {
    let mut trie = PathTrie::default();

    // Stop on the first read error instead of silently filtering it out.
    // A reader that errors without ever reaching EOF (e.g. a directory fd,
    // which returns EISDIR on every read) makes `filter_map(Result::ok)`
    // spin forever re-polling the same failing read -- an unbounded, silent
    // hang rather than a report of what went wrong.
    for line in input.lines() {
        trie.insert(&PathBuf::from(line?), max_level);
    }

    Ok(trie)
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
    }?;

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
