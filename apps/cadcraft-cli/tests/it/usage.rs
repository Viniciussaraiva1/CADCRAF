//! The command line as it behaves today: exit codes, streams and messages for help requests, unknown
//! and malformed options, value flags in both spellings, repeats and arity. Every case goes through
//! `invoke`, so the table rows are the record of what each input does.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Invoke {
    code: i32,
    stdout: String,
    stderr: String,
}

fn invoke(args: &[&str]) -> Invoke {
    let out = Command::new(env!("CARGO_BIN_EXE_cadcraft-cli")).args(args).stdin(Stdio::null()).output().expect("run cadcraft-cli");
    Invoke {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// What a stream holds.
#[derive(Clone, Copy)]
enum Expect {
    Empty,
    /// The usage text.
    Usage,
    /// The first line equals this.
    Line(&'static str),
    /// The first line starts with this.
    Starts(&'static str),
}

use Expect::{Empty, Line, Starts, Usage};

struct Case {
    name: &'static str,
    args: &'static [&'static str],
    code: i32,
    stdout: Expect,
    stderr: Expect,
}

const fn case(name: &'static str, args: &'static [&'static str], code: i32, stdout: Expect, stderr: Expect) -> Case {
    Case { name, args, code, stdout, stderr }
}

/// A scratch directory holding `b.dxf` (the bracket sample) and `s.scr` (a one-line script).
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("cc-cli-usage-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        let b = dir.join("b.dxf");
        let r = invoke(&["sample", "bracket", b.to_str().expect("utf-8 path")]);
        assert_eq!(r.code, 0, "{}", r.stderr);
        std::fs::write(dir.join("s.scr"), "LINE 0,0 1,1\n").expect("write script");
        Scratch(dir)
    }

    /// Replaces `{dir}` with the scratch directory and `{b}` with the sample drawing.
    fn expand(&self, text: &str) -> String {
        let dir = self.0.to_str().expect("utf-8 path");
        text.replace("{b}", &Path::new(dir).join("b.dxf").to_string_lossy()).replace("{dir}", dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn check(what: &str, stream: &str, got: &str, expect: Expect, scratch: &Scratch) {
    let first = got.lines().next().unwrap_or("");
    match expect {
        Empty => assert!(got.is_empty(), "{what}: {stream} should be empty, got {got:?}"),
        Usage => assert!(got.starts_with("usage:"), "{what}: {stream} should be the usage text, got {got:?}"),
        Line(l) => assert_eq!(first, scratch.expand(l), "{what}: {stream}"),
        Starts(p) => assert!(first.starts_with(&scratch.expand(p)), "{what}: {stream} should start with {p:?}, got {first:?}"),
    }
}

fn run_cases(tag: &str, cases: &[Case]) {
    let scratch = Scratch::new(tag);
    for c in cases {
        let args: Vec<String> = c.args.iter().map(|a| scratch.expand(a)).collect();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let what = format!("{}: {args:?}", c.name);
        let r = invoke(&args);
        assert_eq!(r.code, c.code, "{what}: exit code (stderr {:?})", r.stderr);
        check(&what, "stdout", &r.stdout, c.stdout, &scratch);
        check(&what, "stderr", &r.stderr, c.stderr, &scratch);
    }
}

#[test]
fn top_level_requests() {
    run_cases(
        "top",
        &[
            case("--help", &["--help"], 0, Usage, Empty),
            case("-h", &["-h"], 0, Usage, Empty),
            case("help", &["help"], 0, Usage, Empty),
            case("--version", &["--version"], 0, Starts("cadcraft-cli "), Empty),
            case("-V", &["-V"], 0, Starts("cadcraft-cli "), Empty),
            case("bare", &[], 1, Empty, Usage),
            case("unknown subcommand", &["foo"], 1, Empty, Usage),
            case("unknown top-level flag", &["--bogus"], 1, Empty, Usage),
            case("empty subcommand", &[""], 1, Empty, Usage),
        ],
    );
}

#[test]
fn subcommand_help_requests() {
    run_cases(
        "subhelp",
        &[
            case("info --help", &["info", "--help"], 1, Empty, Line("info: unknown option --help")),
            case("info -h is a file name", &["info", "-h"], 1, Empty, Starts("invalid parameters for `open`: -h:")),
            case("convert --help", &["convert", "--help"], 1, Empty, Usage),
            case("convert -h", &["convert", "-h"], 1, Empty, Usage),
            case("run --help", &["run", "--help"], 1, Empty, Line("unknown option --help")),
            case("run -h is a file name", &["run", "-h"], 1, Empty, Starts("invalid parameters for `open`: -h:")),
            case("commands --help", &["commands", "--help"], 1, Empty, Line("commands: unknown option --help")),
            case("mcp --help", &["mcp", "--help"], 1, Empty, Line("mcp: unknown argument --help")),
            case("perf --help", &["perf", "--help"], 1, Empty, Line("perf: unknown option --help")),
            case("sample --help", &["sample", "--help"], 1, Empty, Line("sample: unknown option --help")),
            case("help run", &["help", "run"], 0, Usage, Empty),
            case("help info", &["help", "info"], 0, Usage, Empty),
            case("run --version", &["run", "--version"], 1, Empty, Line("unknown option --version")),
            case("info --version", &["info", "--version"], 1, Empty, Line("info: unknown option --version")),
        ],
    );
}

#[test]
fn unknown_flags_on_every_subcommand() {
    run_cases(
        "unknown",
        &[
            case("info", &["info", "{b}", "--bogus"], 1, Empty, Line("info: unknown option --bogus")),
            case("info, flag first", &["info", "--bogus", "{b}"], 1, Empty, Line("info: unknown option --bogus")),
            case("info, short flag is a file", &["info", "-x"], 1, Empty, Starts("invalid parameters for `open`: -x:")),
            case("convert", &["convert", "{b}", "{dir}/o.svg", "--bogus"], 1, Empty, Usage),
            case("run", &["run", "--sample", "--bogus"], 1, Empty, Line("unknown option --bogus")),
            case("run, flag first", &["run", "--bogus"], 1, Empty, Line("unknown option --bogus")),
            case("commands", &["commands", "--bogus"], 1, Empty, Line("commands: unknown option --bogus")),
            case("commands --", &["commands", "--"], 1, Empty, Line("commands: unknown option --")),
            case("mcp", &["mcp", "--bogus"], 1, Empty, Line("mcp: unknown argument --bogus")),
            case("mcp positional", &["mcp", "extra"], 1, Empty, Line("mcp: unknown argument extra")),
            case("perf", &["perf", "--bogus"], 1, Empty, Line("perf: unknown option --bogus")),
            case("sample", &["sample", "--bogus"], 1, Empty, Line("sample: unknown option --bogus")),
            case("sample after name", &["sample", "bracket", "--bogus"], 1, Empty, Line("sample: unknown option --bogus")),
        ],
    );
}

/// `--x=v` is never a spelling of `--x v`: it is an unknown option, a file name or a usage error.
#[test]
fn value_flags_in_both_forms() {
    run_cases(
        "forms",
        &[
            case("--window v", &["convert", "{b}", "{dir}/w1.svg", "--window", "0,0,100,100"], 0, Empty, Starts("wrote {dir}/w1.svg (")),
            case("--window=v", &["convert", "{b}", "{dir}/w2.svg", "--window=0,0,100,100"], 1, Empty, Usage),
            case(
                "run --window v",
                &["run", "--sample", "--window", "0,0,100,100", "--export", "{dir}/w3.svg"],
                0,
                Empty,
                Starts("wrote {dir}/w3.svg ("),
            ),
            case(
                "run --window=v",
                &["run", "--sample", "--window=0,0,100,100", "--export", "{dir}/w4.svg"],
                1,
                Empty,
                Line("unknown option --window=0,0,100,100"),
            ),
            case("--view v", &["convert", "{b}", "{dir}/v1.svg", "--view", "extents"], 0, Empty, Starts("wrote {dir}/v1.svg (")),
            case("--view=v", &["convert", "{b}", "{dir}/v2.svg", "--view=extents"], 1, Empty, Usage),
            case("run --view v", &["run", "--sample", "--view", "current", "--export", "{dir}/v3.svg"], 0, Empty, Starts("wrote {dir}/v3.svg (")),
            case("run --view=v", &["run", "--sample", "--view=extents"], 1, Empty, Line("unknown option --view=extents")),
            case("--script v", &["run", "--sample", "--script", "LINE 0,0 1,1"], 0, Empty, Line("Command: LINE")),
            case("--script=v", &["run", "--sample", "--script=LINE 0,0 1,1"], 1, Empty, Line("unknown option --script=LINE 0,0 1,1")),
            case("--script-file v", &["run", "--sample", "--script-file", "{dir}/s.scr"], 0, Empty, Starts("Command: LINE")),
            case("--script-file=v", &["run", "--sample", "--script-file={dir}/s.scr"], 1, Empty, Starts("unknown option --script-file=")),
            case("--cmd v", &["run", "--sample", "--cmd", "nosuch"], 1, Empty, Line("unknown command `nosuch`")),
            case("--cmd=v", &["run", "--sample", "--cmd=nosuch"], 1, Empty, Line("unknown option --cmd=nosuch")),
            case("--save v", &["run", "--sample", "--save", "{dir}/s1.dxf"], 0, Empty, Starts("wrote {dir}/s1.dxf (")),
            case("--save=v", &["run", "--sample", "--save={dir}/s2.dxf"], 1, Empty, Starts("unknown option --save=")),
            case("--export v", &["run", "--sample", "--export", "{dir}/e1.svg"], 0, Empty, Starts("wrote {dir}/e1.svg (")),
            case("--export=v", &["run", "--sample", "--export={dir}/e2.svg"], 1, Empty, Starts("unknown option --export=")),
            case("--connect v", &["mcp", "--connect", "127.0.0.1:1"], 1, Empty, Starts("cadcraft-cli: cannot connect to 127.0.0.1:1:")),
            case("--connect=v", &["mcp", "--connect=127.0.0.1:1"], 1, Empty, Line("mcp: unknown argument --connect=127.0.0.1:1")),
        ],
    );
}

/// A value flag at the end of the arguments, or followed by another flag. The value-taking parser
/// consumes the next argument whatever it is, except for `mcp --connect`, which rejects a flag.
#[test]
fn flags_missing_their_value() {
    run_cases(
        "missing",
        &[
            case("convert --window last", &["convert", "{b}", "{dir}/o.svg", "--window"], 1, Empty, Line("--window needs x0,y0,x1,y1")),
            case("convert --view last", &["convert", "{b}", "{dir}/o.svg", "--view"], 1, Empty, Line("--view needs `extents` or `current`")),
            case(
                "convert --window then flag",
                &["convert", "{b}", "{dir}/o.svg", "--window", "--view", "extents"],
                1,
                Empty,
                Line("--window needs four numbers x0,y0,x1,y1, got `--view`"),
            ),
            case(
                "convert --view then flag",
                &["convert", "{b}", "{dir}/o.svg", "--view", "--window", "0,0,1,1"],
                1,
                Empty,
                Line("--view needs `extents` or `current`"),
            ),
            case("run --window last", &["run", "--sample", "--window"], 1, Empty, Line("--window needs x0,y0,x1,y1")),
            case("run --view last", &["run", "--sample", "--view"], 1, Empty, Line("--view needs `extents` or `current`")),
            case("run --script last", &["run", "--sample", "--script"], 1, Empty, Line("--script needs text")),
            case("run --script then flag", &["run", "--sample", "--script", "--sample"], 1, Empty, Line("unknown command `--sample`")),
            case("run --script-file last", &["run", "--script-file"], 1, Empty, Line("--script-file needs a path")),
            case("run --script-file then flag", &["run", "--sample", "--script-file", "--sample"], 1, Empty, Starts("--sample:")),
            case("run --cmd last", &["run", "--sample", "--cmd"], 1, Empty, Line("--cmd needs `id {json}`")),
            case("run --cmd then flag", &["run", "--sample", "--cmd", "--sample"], 1, Empty, Line("unknown command `--sample`")),
            case("run --save last", &["run", "--sample", "--save"], 1, Empty, Line("--save needs a path")),
            case("run --export last", &["run", "--sample", "--export"], 1, Empty, Line("--export needs a path")),
            case("mcp --connect last", &["mcp", "--connect"], 1, Empty, Starts("--connect needs HOST:PORT")),
            case("mcp --connect then flag", &["mcp", "--connect", "--lazy"], 1, Empty, Starts("--connect needs HOST:PORT")),
        ],
    );
}

#[test]
fn repeated_and_combined_flags() {
    run_cases(
        "repeat",
        &[
            case(
                "--save twice: the last wins",
                &["run", "--sample", "--save", "{dir}/a.dxf", "--save", "{dir}/b2.dxf"],
                0,
                Empty,
                Starts("wrote {dir}/b2.dxf ("),
            ),
            case(
                "--export twice: the last wins",
                &["run", "--sample", "--export", "{dir}/x1.svg", "--export", "{dir}/x2.svg"],
                0,
                Empty,
                Starts("wrote {dir}/x2.svg ("),
            ),
            case(
                "--window twice: the last wins",
                &["run", "--sample", "--window", "0,0,1,1", "--window", "0,0,2,2", "--export", "{dir}/ww.svg"],
                0,
                Empty,
                Starts("wrote {dir}/ww.svg ("),
            ),
            case(
                "convert --window twice",
                &["convert", "{b}", "{dir}/cw.svg", "--window", "0,0,1,1", "--window", "0,0,2,2"],
                0,
                Empty,
                Starts("wrote {dir}/cw.svg ("),
            ),
            case(
                "--view twice, different",
                &["run", "--sample", "--view", "extents", "--view", "current", "--export", "{dir}/vv.svg"],
                1,
                Empty,
                Line("use only one of --window and --view"),
            ),
            case(
                "--view twice, same",
                &["run", "--sample", "--view", "extents", "--view", "extents", "--export", "{dir}/vs.svg"],
                0,
                Empty,
                Starts("wrote {dir}/vs.svg ("),
            ),
            case(
                "--window and --view",
                &["run", "--sample", "--window", "0,0,1,1", "--view", "extents", "--export", "{dir}/wv.svg"],
                1,
                Empty,
                Line("use only one of --window and --view"),
            ),
            case(
                "convert --window and --view",
                &["convert", "{b}", "{dir}/cwv.svg", "--window", "0,0,1,1", "--view", "extents"],
                1,
                Empty,
                Line("use only one of --window and --view"),
            ),
            case(
                "--view is case-insensitive",
                &["run", "--sample", "--view", "EXTENTS", "--export", "{dir}/vu.svg"],
                0,
                Empty,
                Starts("wrote {dir}/vu.svg ("),
            ),
            case(
                "--connect twice",
                &["mcp", "--connect", "127.0.0.1:1", "--connect", "127.0.0.1:2"],
                1,
                Empty,
                Line("--connect given more than once"),
            ),
            case("--lazy without --connect", &["mcp", "--lazy"], 1, Empty, Line("--lazy and --launch need --connect HOST:PORT")),
            case("--launch without --connect", &["mcp", "--launch"], 1, Empty, Line("--lazy and --launch need --connect HOST:PORT")),
            case("--sample --metric: both accepted", &["run", "--sample", "--metric"], 0, Empty, Empty),
            case("--sample and a file", &["run", "--sample", "{b}"], 0, Empty, Empty),
            case("two files", &["run", "{b}", "{b}"], 0, Empty, Empty),
            case("framing without export", &["run", "--sample", "--window", "0,0,1,1"], 1, Empty, Starts("--window/--view need an image output")),
            case("--view without export", &["run", "--sample", "--view", "current"], 1, Empty, Starts("--window/--view need an image output")),
            case(
                "framing with a dxf output",
                &["convert", "{b}", "{dir}/o.dxf", "--window", "0,0,1,1"],
                1,
                Empty,
                Starts("--window/--view only apply to image output"),
            ),
        ],
    );
}

#[test]
fn arity_and_positional_arguments() {
    run_cases(
        "arity",
        &[
            case("info without a file", &["info"], 1, Empty, Usage),
            case("info with two files: the second is ignored", &["info", "{b}", "extra"], 0, Starts("{"), Empty),
            case("info missing file", &["info", "missing.dxf"], 1, Empty, Starts("invalid parameters for `open`: missing.dxf:")),
            case("convert with no files", &["convert"], 1, Empty, Usage),
            case("convert with one file", &["convert", "{b}"], 1, Empty, Usage),
            case("convert with three files", &["convert", "{b}", "{dir}/o.svg", "{dir}/x.svg"], 1, Empty, Usage),
            case("run missing file", &["run", "missing.dxf"], 1, Empty, Starts("invalid parameters for `open`: missing.dxf:")),
            case("commands, no match", &["commands", "zzzznomatch"], 0, Line("[]"), Empty),
            case("commands, extra argument ignored", &["commands", "zzzznomatch", "extra"], 0, Line("[]"), Empty),
            case("sample without arguments", &["sample"], 1, Empty, Usage),
            case("sample without an output", &["sample", "bracket"], 1, Empty, Usage),
            case("sample with a bad name", &["sample", "nosuch", "{dir}/o.dxf"], 1, Empty, Line("unknown sample `nosuch` (bracket, floorplan)")),
            case("sample with an extra argument", &["sample", "bracket", "{dir}/sx.dxf", "extra"], 0, Empty, Starts("wrote {dir}/sx.dxf (")),
            case("sample floor alias", &["sample", "floor", "{dir}/sf.dxf"], 0, Empty, Starts("wrote {dir}/sf.dxf (")),
            case("sample with a bad extension", &["sample", "bracket", "{dir}/o.xyz"], 1, Empty, Line("unsupported file type `xyz`")),
            case("perf with a word", &["perf", "abc"], 1, Empty, Starts("invalid parameters for `open`: abc:")),
            case("perf with a negative number", &["perf", "-5"], 1, Empty, Starts("invalid parameters for `open`: -5:")),
            case("perf with a missing file", &["perf", "missing.dxf"], 1, Empty, Starts("invalid parameters for `open`: missing.dxf:")),
            case("perf 1", &["perf", "1"], 0, Starts("operation"), Empty),
            case("perf 0 is raised to 1", &["perf", "0"], 0, Starts("operation"), Empty),
            case("perf with two counts: the second is ignored", &["perf", "1", "2"], 0, Starts("operation"), Empty),
            case("mcp headless with a closed stdin", &["mcp"], 0, Empty, Empty),
        ],
    );
}

#[test]
fn bad_values() {
    run_cases(
        "values",
        &[
            case("--view sideways", &["convert", "{b}", "{dir}/o.svg", "--view", "sideways"], 1, Empty, Line("--view needs `extents` or `current`")),
            case("run --view sideways", &["run", "--sample", "--view", "sideways"], 1, Empty, Line("--view needs `extents` or `current`")),
            case(
                "--view sideways before the output check",
                &["convert", "{b}", "{dir}/o.dxf", "--view", "sideways"],
                1,
                Empty,
                Line("--view needs `extents` or `current`"),
            ),
            case(
                "--window 3 numbers",
                &["convert", "{b}", "{dir}/o.svg", "--window", "1,2,3"],
                1,
                Empty,
                Line("--window needs four numbers x0,y0,x1,y1, got `1,2,3`"),
            ),
            case(
                "--window 5 numbers",
                &["convert", "{b}", "{dir}/o.svg", "--window", "1,2,3,4,5"],
                1,
                Empty,
                Line("--window needs four numbers x0,y0,x1,y1, got `1,2,3,4,5`"),
            ),
            case(
                "--window reversed",
                &["convert", "{b}", "{dir}/o.svg", "--window", "3,3,1,1"],
                1,
                Empty,
                Line("--window: need x0 < x1 and y0 < y1 (lower-left corner first), got 3,3,1,1"),
            ),
            case(
                "--window not numbers",
                &["convert", "{b}", "{dir}/o.svg", "--window", "a,b,c,d"],
                1,
                Empty,
                Line("--window: x0 must be a finite number, got `a`"),
            ),
            case(
                "--window NaN",
                &["convert", "{b}", "{dir}/o.svg", "--window", "NaN,0,1,1"],
                1,
                Empty,
                Line("--window: x0 must be a finite number, got `NaN`"),
            ),
            case(
                "run --window 3 numbers",
                &["run", "--sample", "--window", "1,2,3", "--export", "{dir}/o.svg"],
                1,
                Empty,
                Line("--window needs four numbers x0,y0,x1,y1, got `1,2,3`"),
            ),
            case(
                "run --window reversed",
                &["run", "--sample", "--window", "3,3,1,1", "--export", "{dir}/o.svg"],
                1,
                Empty,
                Line("--window: need x0 < x1 and y0 < y1 (lower-left corner first), got 3,3,1,1"),
            ),
            case(
                "--cmd bad json",
                &["run", "--sample", "--cmd", "line {bad json"],
                1,
                Empty,
                Line("--cmd line: key must be a string at line 1 column 2"),
            ),
            case("--cmd unknown id", &["run", "--sample", "--cmd", "nosuch"], 1, Empty, Line("unknown command `nosuch`")),
            case("--script-file missing", &["run", "--sample", "--script-file", "missing.scr"], 1, Empty, Starts("missing.scr:")),
        ],
    );
}
