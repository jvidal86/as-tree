use std::env;
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
pub enum Colorize {
    Always,
    Auto,
    Never,
}

impl FromStr for Colorize {
    type Err = ();

    fn from_str(color: &str) -> Result<Self, Self::Err> {
        match color {
            "always" => Ok(Colorize::Always),
            "auto" => Ok(Colorize::Auto),
            "never" => Ok(Colorize::Never),
            _ => Err(()),
        }
    }
}

impl Default for Colorize {
    fn default() -> Self {
        Colorize::Auto
    }
}

#[derive(Debug, Default)]
pub struct Options {
    pub filename: Option<PathBuf>,
    pub colorize: Colorize,
    pub full_path: bool,
    // Maximum tree depth to print, counted in path components from the top.
    // None means unlimited.
    pub max_level: Option<usize>,
}

const USAGE: &str = "\
Print a list of paths as a tree of paths.

Usage:
  as-tree [options] [<filename>]

Arguments:
  <filename>        The file to read from. When omitted, reads from stdin.

Options:
  --color (always|auto|never)
                    Whether to colorize the output [default: auto]
  -f                Prints the full path prefix for each file.
  -L <level>        Descend only <level> levels deep (must be > 0)
  -h, --help        Print this help message
  -v, --version     Print the version and exit

Example:
  find . -name '*.txt' | as-tree
";

pub fn parse_options_or_die() -> Options {
    fn die(msg: &str, arg: &str) -> ! {
        eprint!("{} '{}'\n\n{}", msg, arg, USAGE);
        exit(1);
    }

    // `env::args()` panics if any argument isn't valid Unicode, which is a
    // real crash on Linux/macOS: argv is raw bytes with no encoding enforced
    // by the kernel, so a legacy-encoded or crafted filename can crash the
    // parser before any of our own validation runs. `args_os()` never panics;
    // flags are always plain ASCII, so a non-UTF-8 argument simply can't match
    // one and falls through to being treated as the filename, which itself
    // does not require valid Unicode either (`File::open` accepts raw OS
    // strings via `AsRef<Path>`).
    let mut argv = env::args_os();

    if argv.next().is_none() {
        eprint!("{}", USAGE);
        exit(1);
    }

    let mut options = Options::default();
    while let Some(arg) = argv.next() {
        if arg.is_empty() {
            die("Unrecognized argument:", &arg.to_string_lossy());
        }

        let arg_str = arg.to_str();

        if arg_str == Some("-h") || arg_str == Some("--help") {
            print!("{}", USAGE);
            exit(0);
        }

        if arg_str == Some("-v") || arg_str == Some("--version") {
            println!("{}", VERSION);
            exit(0);
        }

        if arg_str == Some("-f") {
            options.full_path = true;
            continue;
        }

        if arg_str == Some("--color") {
            if let Some(color) = argv.next() {
                match color.to_str().and_then(|c| c.parse().ok()) {
                    Some(colorize) => options.colorize = colorize,
                    None => die("Unrecognized option: --color", &color.to_string_lossy()),
                }
            } else {
                die("-> Unrecognized option:", "--color");
            }
            continue;
        }

        if arg_str == Some("-L") {
            match argv.next() {
                Some(level) => match level.to_str().map(str::parse::<usize>) {
                    Some(Ok(n)) if n > 0 => options.max_level = Some(n),
                    Some(Ok(_)) => {
                        die("Invalid level, must be greater than 0:", &level.to_string_lossy())
                    }
                    _ => die("Invalid level for -L:", &level.to_string_lossy()),
                },
                None => die("Missing value for -L:", "-L"),
            }
            continue;
        }

        if arg_str.map(|s| s.starts_with('-')).unwrap_or(false) {
            die("Unrecognized option:", &arg.to_string_lossy());
        }

        if options.filename.is_some() {
            die("Extra argument:", &arg.to_string_lossy());
        }

        options.filename = Some(PathBuf::from(arg));
    }

    options
}
