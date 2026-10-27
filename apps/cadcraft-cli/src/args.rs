//! The command line, as clap types. Values stay strings: the handlers check them, with their own
//! messages. A flag a subcommand doesn't take is a usage error (exit 2), so a typo can't be
//! silently ignored. A value flag never takes the next flag as its value: a value starting with `-`
//! is written `--save=-out.dxf` (or `./-out.dxf`).

use std::ffi::OsString;

use clap::parser::ValueSource;
use clap::{ArgAction, ArgMatches, Args, CommandFactory, FromArgMatches, Parser, Subcommand};

use super::*;

/// `cadcraft-cli <version> (<commit>)`, the desktop app's `--version` text with this binary's name.
fn long_version() -> &'static str {
    static LONG: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| format!("{} ({})", env!("CARGO_PKG_VERSION"), option_env!("CADCRAFT_BUILD_SHA").unwrap_or("dev")));
    &LONG
}

/// cadcraft-cli: headless CADCraft
#[derive(Parser)]
#[command(name = "cadcraft-cli", version = long_version(), arg_required_else_help = true)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    Info(InfoArgs),
    Convert(ConvertArgs),
    Run(RunArgs),
    Commands(CommandsArgs),
    Mcp(McpArgs),
    Perf(PerfArgs),
    Sample(SampleArgs),
}

/// Print a drawing's summary as JSON
#[derive(Args)]
pub(crate) struct InfoArgs {
    /// The drawing to read (dxf, dwg)
    pub(crate) file: String,
}

/// Convert a drawing; OUT's extension picks the format (dxf, dwg, svg, png, pdf)
///
/// Image output (png 2400x1600, svg, pdf) shows model space, fitted to the drawing extents unless
/// --window or --view says otherwise.
#[derive(Args)]
pub(crate) struct ConvertArgs {
    /// The drawing to read
    #[arg(value_name = "IN")]
    pub(crate) input: String,
    /// The file to write
    #[arg(value_name = "OUT")]
    pub(crate) output: String,
    #[command(flatten)]
    pub(crate) framing: FramingArgs,
}

// What part of model space an image shows: shared by `convert` and `run`.
#[derive(Args)]
pub(crate) struct FramingArgs {
    /// Show this rectangle (drawing units, x0<x1, y0<y1), centred and scaled to fit the image
    /// without distortion. A negative x0 needs the `=` form: --window=-10,0,10,5
    #[arg(long, value_name = "x0,y0,x1,y1")]
    pub(crate) window: Option<String>,
    /// extents: always fit the drawing extents. current: the current view, as last set by zoom,
    /// pan or view.set (a freshly opened drawing's view is its extents plus a 10% margin)
    #[arg(long, value_name = "extents|current")]
    pub(crate) view: Option<String>,
}

/// Open drawings (or start a new one), run scripts and commands in order, save or export the result
///
/// Drawings open in the order given, and the last one is current; without any, a new imperial
/// drawing is used. --script, --script-file and --cmd steps run in the order given, and each
/// --cmd prints its JSON result. An image --export (png 2400x1600, svg, pdf) shows model space:
/// the current view if the steps changed it (zoom, pan, view.set…), otherwise the drawing
/// extents, unless --window or --view says otherwise.
#[derive(Args)]
#[command(after_long_help = r#"--cmd chaining:
  A --cmd parameter value of exactly "{last}" is replaced with the last object handle an
  earlier --cmd in the same run returned, so a command can act on what the previous one made:
    --cmd 'mview {"p1":[20,20],"p2":[200,150]}'        prints {"viewports":["100"]}
    --cmd 'viewport.set {"handle":"{last}","scale":1}' changes viewport 100
  It works in any parameter, at any depth (e.g. "handles":["{last}"]); every other string is
  passed unchanged. With no earlier returned handle the run fails and says so."#)]
pub(crate) struct RunArgs {
    /// Drawings to open
    #[arg(value_name = "FILE")]
    pub(crate) files: Vec<String>,
    /// Open the built-in sample drawing
    #[arg(long)]
    pub(crate) sample: bool,
    /// Start a new metric drawing
    #[arg(long)]
    pub(crate) metric: bool,
    /// Command-line script text to run (`\n` separates lines)
    #[arg(long, value_name = "TEXT", action = ArgAction::Append)]
    pub(crate) script: Vec<String>,
    /// A script file (.scr) to run
    #[arg(long, value_name = "F.scr", action = ArgAction::Append)]
    pub(crate) script_file: Vec<String>,
    /// An engine command and its JSON parameters (default {}); see "--cmd chaining" in --help
    #[arg(long, value_name = "id {json}", action = ArgAction::Append)]
    pub(crate) cmd: Vec<String>,
    /// Save the drawing to this file
    #[arg(long, value_name = "OUT.dxf")]
    pub(crate) save: Option<String>,
    /// Export the drawing to this file (png, svg, pdf, or a drawing format)
    #[arg(long, value_name = "OUT.png")]
    pub(crate) export: Option<String>,
    #[command(flatten)]
    pub(crate) framing: FramingArgs,
    /// The drawings, in command-line order, set by [`parse`].
    #[arg(skip)]
    pub(crate) drawings: Vec<Drawing>,
    /// The steps, in command-line order, set by [`parse`].
    #[arg(skip)]
    pub(crate) steps: Vec<Step>,
}

/// A drawing `run` opens.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Drawing {
    File(String),
    Sample,
    Metric,
}

/// A `--script`, `--script-file` or `--cmd` of `run`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    Script(String),
    ScriptFile(String),
    Cmd(String),
}

/// Print the command catalog as JSON
#[derive(Args)]
pub(crate) struct CommandsArgs {
    /// Only the commands whose id or label contains this text (case-insensitive)
    pub(crate) filter: Option<String>,
}

/// Run the MCP server on stdio: a headless drawing session, or a bridge to the running app
#[derive(Args)]
pub(crate) struct McpArgs {
    /// Bridge to the app's control channel (its --control port, e.g. 127.0.0.1:7979)
    #[arg(long, value_name = "HOST:PORT", value_parser = connect_addr)]
    pub(crate) connect: Option<String>,
    /// Reach the app at the first call instead of at startup, so the server can start before the
    /// app is open
    #[arg(long, requires = "connect")]
    pub(crate) lazy: bool,
    /// Like --lazy, and a call that finds nothing at HOST:PORT opens the app with that control
    /// port first (macOS)
    #[arg(long, requires = "connect")]
    pub(crate) launch: bool,
}

/// Time the engine on a synthetic N-entity drawing (default 200000), or the canvas display-list
/// build of a drawing, zoomed in step by step
#[derive(Args)]
pub(crate) struct PerfArgs {
    /// An entity count, or a drawing to time
    #[arg(value_name = "N|FILE")]
    pub(crate) target: Option<String>,
}

/// Write a built-in sample drawing
#[derive(Args)]
pub(crate) struct SampleArgs {
    /// Which sample: bracket or floorplan
    #[arg(value_name = "bracket|floorplan")]
    pub(crate) name: String,
    /// The file to write; its extension picks the format (dxf, dwg, svg, png)
    #[arg(value_name = "OUT")]
    pub(crate) output: String,
}

/// What part of model space an image export shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Framing {
    /// `run`: the current view if the commands changed it, else the extents. `convert`: extents.
    Auto,
    Extents,
    Current,
    Window(Bounds2),
}

/// Why a `--window` value is rejected.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WindowError {
    #[error("--window needs four numbers x0,y0,x1,y1, got `{0}`")]
    NeedsFour(String),
    #[error("--window: {name} must be a finite number, got `{value}`")]
    NotFinite { name: &'static str, value: String },
    #[error("--window: need x0 < x1 and y0 < y1 (lower-left corner first), got {x0},{y0},{x1},{y1}")]
    Order { x0: f64, y0: f64, x1: f64, y1: f64 },
    #[error("--window: {0}")]
    Limits(#[from] cadcraft_io::IoError),
}

/// Why the framing options are rejected.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ArgsError {
    #[error("--view needs `extents` or `current`")]
    ViewNeedsValue,
    #[error("use only one of --window and --view")]
    FramingConflict,
    #[error("--window/--view only apply to image output (png, svg, pdf), not `{0}`")]
    FramingNeedsImage(String),
    #[error("--window/--view need an image output (--export OUT.png|svg|pdf)")]
    FramingNeedsExport,
}

/// Why a value is refused: clap prints it after the flag and its value (exit 2).
#[derive(Debug, thiserror::Error)]
pub(crate) enum ValueError {
    #[error("expected HOST:PORT (the app's --control port, e.g. 127.0.0.1:7979)")]
    Connect,
}

/// Parse `--window x0,y0,x1,y1` (min corner first). Hostile values are errors, never panics.
pub(crate) fn parse_window(text: &str) -> Result<Bounds2, WindowError> {
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    let [x0, y0, x1, y1] = parts.as_slice() else {
        return Err(WindowError::NeedsFour(text.to_string()));
    };
    let num = |name: &'static str, v: &str| -> Result<f64, WindowError> {
        match v.parse::<f64>() {
            Ok(n) if n.is_finite() => Ok(n),
            _ => Err(WindowError::NotFinite { name, value: v.to_string() }),
        }
    };
    let (x0, y0, x1, y1) = (num("x0", x0)?, num("y0", y0)?, num("x1", x1)?, num("y1", y1)?);
    if !(x0 < x1 && y0 < y1) {
        return Err(WindowError::Order { x0, y0, x1, y1 });
    }
    let w = Bounds2 { min: Vec2::new(x0, y0), max: Vec2::new(x1, y1) };
    cadcraft_io::check_window(&w)?;
    Ok(w)
}

impl FramingArgs {
    /// The framing `--window` or `--view` asks for; `--view` is case-insensitive.
    pub(crate) fn framing(&self) -> Result<Framing> {
        let window = self.window.as_deref().map(parse_window).transpose()?;
        let view = match self.view.as_deref().map(str::to_ascii_lowercase).as_deref() {
            None => None,
            Some("extents") => Some(Framing::Extents),
            Some("current") => Some(Framing::Current),
            Some(_) => return Err(ArgsError::ViewNeedsValue.into()),
        };
        match (window, view) {
            (Some(_), Some(_)) => Err(ArgsError::FramingConflict.into()),
            (Some(w), None) => Ok(Framing::Window(w)),
            (None, Some(v)) => Ok(v),
            (None, None) => Ok(Framing::Auto),
        }
    }
}

/// Reject an explicit framing option without an image output to apply it to.
pub(crate) fn check_framed_output(framing: Framing, out: Option<&str>) -> Result<(), ArgsError> {
    match out {
        _ if framing == Framing::Auto => Ok(()),
        Some(o) if !cadcraft_io::is_image(o) => Err(ArgsError::FramingNeedsImage(o.to_string())),
        Some(_) => Ok(()),
        None => Err(ArgsError::FramingNeedsExport),
    }
}

/// The backend `cadcraft-cli mcp` serves from.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum McpTarget {
    /// An in-process drawing session (no `--connect`).
    Headless,
    /// The running app's control channel at `HOST:PORT`; `lazy`: reached at the first call;
    /// `launch`: opened then if it isn't running.
    Connect { addr: String, lazy: bool, launch: bool },
}

impl McpArgs {
    pub(crate) fn target(&self) -> McpTarget {
        match &self.connect {
            // `--launch` connects at the first call too (the app opens then).
            Some(addr) => McpTarget::Connect { addr: addr.clone(), lazy: self.lazy || self.launch, launch: self.launch },
            None => McpTarget::Headless,
        }
    }
}

/// `mcp --connect`: an empty or flag-like address is an error, never a silent fall back to a
/// headless session (#103).
fn connect_addr(s: &str) -> Result<String, ValueError> {
    let s = s.trim();
    if s.is_empty() || s.starts_with('-') { Err(ValueError::Connect) } else { Ok(s.to_string()) }
}

/// The values of `id` with their command-line indices.
fn indexed<'a>(m: &'a ArgMatches, id: &str) -> impl Iterator<Item = (usize, &'a String)> {
    let at = m.indices_of(id).into_iter().flatten();
    let values = m.try_get_many::<String>(id).ok().flatten().into_iter().flatten();
    at.zip(values)
}

/// The command-line index of flag `id`, when it was given.
fn flag_index(m: &ArgMatches, id: &str) -> Option<usize> {
    if m.value_source(id) == Some(ValueSource::CommandLine) { m.indices_of(id).and_then(Iterator::last) } else { None }
}

/// `run`'s drawings and steps back in command-line order: clap keeps each flag's values apart,
/// their indices say how they interleave.
fn run_order(m: &ArgMatches) -> (Vec<Drawing>, Vec<Step>) {
    let mut drawings: Vec<(usize, Drawing)> = indexed(m, "files").map(|(i, f)| (i, Drawing::File(f.clone()))).collect();
    drawings.extend(flag_index(m, "sample").map(|i| (i, Drawing::Sample)));
    drawings.extend(flag_index(m, "metric").map(|i| (i, Drawing::Metric)));
    drawings.sort_by_key(|d| d.0);
    let mut steps: Vec<(usize, Step)> = indexed(m, "script").map(|(i, s)| (i, Step::Script(s.clone()))).collect();
    steps.extend(indexed(m, "script_file").map(|(i, s)| (i, Step::ScriptFile(s.clone()))));
    steps.extend(indexed(m, "cmd").map(|(i, s)| (i, Step::Cmd(s.clone()))));
    steps.sort_by_key(|s| s.0);
    (drawings.into_iter().map(|d| d.1).collect(), steps.into_iter().map(|s| s.1).collect())
}

/// Parse the arguments (without the binary name). clap reads them as `OsString`s; a value that is
/// not valid Unicode is a usage error. A bare `--help` or `-h` is help even after a mistake on the
/// same line (`run --script --help`): clap reports the mistake first.
pub(crate) fn parse(args: &[OsString]) -> Result<Cli, clap::Error> {
    let mut cmd = Cli::command();
    let argv = std::iter::once(OsString::from("cadcraft-cli")).chain(args.iter().cloned());
    let matches = match cmd.try_get_matches_from_mut(argv) {
        // `use_stderr`: a real error, not the help `-h` asked for (a retry would make it `--help`).
        Err(e) if e.use_stderr() && args.iter().take_while(|a| *a != "--").any(|a| *a == "--help" || *a == "-h") => {
            let help = [OsString::from("cadcraft-cli"), args.first().cloned().unwrap_or_default(), OsString::from("--help")];
            return Err(cmd.try_get_matches_from_mut(help).err().unwrap_or(e));
        }
        matches => matches?,
    };
    let mut cli = Cli::from_arg_matches(&matches)?;
    if let (Command::Run(r), Some((_, m))) = (&mut cli.command, matches.subcommand()) {
        (r.drawings, r.steps) = run_order(m);
    }
    Ok(cli)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(a: &[&str]) -> Result<Cli, clap::Error> {
        parse(&a.iter().map(OsString::from).collect::<Vec<_>>())
    }

    fn mcp(a: &[&str]) -> Result<McpTarget, clap::Error> {
        let args: Vec<&str> = std::iter::once("mcp").chain(a.iter().copied()).collect();
        match parse_args(&args)?.command {
            Command::Mcp(m) => Ok(m.target()),
            _ => panic!("{a:?} is not mcp"),
        }
    }

    #[test]
    fn mcp_without_connect_is_headless() {
        assert_eq!(mcp(&[]).unwrap(), McpTarget::Headless);
    }

    #[test]
    fn mcp_connect_with_address_selects_remote() {
        let to = |addr: &str, lazy, launch| McpTarget::Connect { addr: addr.into(), lazy, launch };
        assert_eq!(mcp(&["--connect", "127.0.0.1:7979"]).unwrap(), to("127.0.0.1:7979", false, false));
        assert_eq!(mcp(&["--connect", " localhost:7979 "]).unwrap(), to("localhost:7979", false, false));
        assert_eq!(mcp(&["--lazy", "--connect", "localhost:7979"]).unwrap(), to("localhost:7979", true, false));
        assert_eq!(mcp(&["--connect", "localhost:7979", "--launch"]).unwrap(), to("localhost:7979", true, true));
        // Without an app to reach they mean nothing: never a headless session.
        for flag in ["--lazy", "--launch"] {
            assert_eq!(mcp(&[flag]).unwrap_err().kind(), clap::error::ErrorKind::MissingRequiredArgument, "{flag}");
        }
    }

    #[test]
    fn mcp_connect_without_address_is_an_error_not_headless() {
        for bad in [&["--connect"][..], &["--connect", ""], &["--connect", "  "], &["--connect=-v"]] {
            let r = mcp(bad);
            let Err(e) = r else { panic!("{bad:?} should be rejected, got {r:?}") };
            assert_eq!(e.exit_code(), 2, "{bad:?}");
            assert!(e.to_string().contains("--connect <HOST:PORT>"), "{bad:?}: {e}");
        }
    }

    #[test]
    fn mcp_rejects_unknown_and_repeated_arguments() {
        assert!(mcp(&["--conect", "127.0.0.1:7979"]).is_err());
        assert!(mcp(&["127.0.0.1:7979"]).is_err());
        // clap names the unknown flag, not the value it left missing.
        for flag in ["--foo", "-v"] {
            assert_eq!(mcp(&["--connect", flag]).unwrap_err().kind(), clap::error::ErrorKind::UnknownArgument, "{flag}");
        }
        assert!(mcp(&["--connect", "127.0.0.1:1", "--connect", "127.0.0.1:2"]).is_err());
    }

    #[test]
    fn run_keeps_command_line_order() {
        let args = ["run", "a.dxf", "--script", "S1", "--metric", "--cmd", "c1", "b.dxf", "--script-file", "f.scr", "--sample", "--cmd", "c2"];
        let Command::Run(r) = parse_args(&args).unwrap().command else { panic!("not run") };
        let file = |f: &str| Drawing::File(f.into());
        assert_eq!(r.drawings, [file("a.dxf"), Drawing::Metric, file("b.dxf"), Drawing::Sample]);
        assert_eq!(r.steps, [Step::Script("S1".into()), Step::Cmd("c1".into()), Step::ScriptFile("f.scr".into()), Step::Cmd("c2".into())]);
    }

    #[test]
    fn framing_options() {
        let framing = |a: &[&str]| -> Result<Framing> {
            let args: Vec<&str> = ["convert", "a.dxf", "b.png"].iter().chain(a).copied().collect();
            let Command::Convert(c) = parse_args(&args).unwrap().command else { panic!("not convert") };
            c.framing.framing()
        };
        let b = |x0, y0, x1, y1| Bounds2::new(Vec2::new(x0, y0), Vec2::new(x1, y1));
        assert_eq!(framing(&[]).unwrap(), Framing::Auto);
        assert_eq!(framing(&["--view", "extents"]).unwrap(), Framing::Extents);
        assert_eq!(framing(&["--view", "CURRENT"]).unwrap(), Framing::Current);
        assert_eq!(framing(&["--window", "0,0,2,1"]).unwrap(), Framing::Window(b(0.0, 0.0, 2.0, 1.0)));
        assert!(framing(&["--view", "selection"]).is_err());
        assert!(framing(&["--window", "0,0,2,1", "--view", "extents"]).is_err());
        assert!(check_framed_output(Framing::Extents, Some("a.dxf")).is_err());
        assert!(check_framed_output(Framing::Extents, None).is_err());
        assert!(check_framed_output(Framing::Extents, Some("a.png")).is_ok());
        assert!(check_framed_output(Framing::Auto, Some("a.dxf")).is_ok());
    }
}
