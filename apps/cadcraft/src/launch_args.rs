//! The desktop app's command line, as clap types: `cadcraft --help` comes from these doc comments,
//! so the help and the parser can't disagree. Anything they don't declare is a usage error naming
//! it (exit status 2), so a typo such as `--sampel` doesn't open the window and then fail to open
//! the flag as a file; so is a flag without its value, a repeated flag and a bad port, from the
//! flag or from `CADCRAFT_CONTROL_PORT`. A value never starts with `-`, and a file whose name
//! starts with `-` goes after `--`.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Arg, Command, CommandFactory, FromArgMatches, Parser};

/// Opens CADCraft with the given files
#[derive(Parser)]
#[command(name = "cadcraft", version = crate::build_version(), after_help = ENVIRONMENT)]
pub(crate) struct LaunchArgs {
    /// Files to open. Put `--` before a name that starts with `-`
    pub(crate) files: Vec<PathBuf>,
    /// Start the localhost JSON-lines control server on this port
    // allow_negative_numbers makes `--control -1` a bad value rather than a missing one.
    #[arg(long, value_name = "PORT", env = "CADCRAFT_CONTROL_PORT", value_parser = control_port, allow_negative_numbers = true)]
    pub(crate) control: Option<u16>,
    /// Open with the sample drawing
    #[arg(long)]
    pub(crate) sample: bool,
}

/// The variables that aren't a flag's fallback, for the end of `--help`.
const ENVIRONMENT: &str = "\
Environment:
  CADCRAFT_CONFIG_DIR       the settings folder (logs, autosave, plotters)
  CADCRAFT_LOCALE           the interface language, instead of the system's
  CADCRAFT_VSYNC            1 presents with vsync on Linux and BSD, 0 without it elsewhere
  CADCRAFT_WAYLAND          1 keeps the native Wayland backend instead of XWayland
  CADCRAFT_NO_NATIVE_MENU   macOS: any value keeps the menus inside the window
  CADCRAFT_SYSTEM_CURSOR    1 uses the system cursor instead of the drawn crosshair
  CADCRAFT_FONTALT          the font that replaces a missing one
  CADCRAFT_FONTFALLBACK     the fonts tried, in order, for characters a font lacks";

/// The files to open, as [`LaunchArgs::file_names`] sorts them.
#[derive(Debug, PartialEq)]
pub(crate) struct FileNames {
    pub(crate) openable: Vec<String>,
    /// Names that are not valid Unicode, shown lossily: reported, not opened.
    pub(crate) unreadable: Vec<String>,
}

impl LaunchArgs {
    /// The files to open, and those whose names are not valid Unicode: a file manager can hand
    /// over a Latin-1 name on Linux. `CadApp::open_path` takes text, so those are only reported.
    pub(crate) fn file_names(&self) -> FileNames {
        let mut names = FileNames { openable: Vec::new(), unreadable: Vec::new() };
        for f in &self.files {
            match f.to_str() {
                Some(s) => names.openable.push(s.to_owned()),
                None => names.unreadable.push(f.to_string_lossy().into_owned()),
            }
        }
        names
    }
}

/// The command line. An empty or blank variable counts as unset.
pub(crate) fn command() -> Command {
    LaunchArgs::command().mut_args(blank_env_is_unset)
}

/// Parse the launch arguments (without the binary name) with `cmd`, from [`command`]. Old macOS
/// passes a process serial number (`-psn_…`) when launched from Finder: it is dropped first, unless
/// it comes after `--`. A bad value from a variable is a usage error naming the variable.
pub(crate) fn parse(mut cmd: Command, args: impl IntoIterator<Item = OsString>) -> Result<LaunchArgs, clap::Error> {
    let mut options = true;
    let args: Vec<OsString> = args
        .into_iter()
        .filter(|a| {
            options &= a != "--";
            !(options && a.to_str().is_some_and(|s| s.starts_with("-psn_")))
        })
        .collect();
    let argv = std::iter::once(OsString::from("cadcraft")).chain(args.iter().cloned());
    let matches = cmd.try_get_matches_from_mut(argv).map_err(|e| name_env_vars(e, &cmd, &args))?;
    LaunchArgs::from_arg_matches(&matches)
}

/// A control port, for clap's `value_parser`: `--control` and `CADCRAFT_CONTROL_PORT`, which may
/// carry spaces.
fn control_port(s: &str) -> Result<u16, NotAPort> {
    s.trim().parse().map_err(|_| NotAPort)
}

/// A [`control_port`] value that isn't a port.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("expected a port, 0-65535")]
struct NotAPort;

/// For `Command::mut_args`: an empty or blank variable counts as unset, so `CADCRAFT_CONTROL_PORT=`
/// turns a fallback off without `unset`. clap would pass the empty value to the parser.
fn blank_env_is_unset(arg: Arg) -> Arg {
    let blank = arg.get_env().and_then(std::env::var_os).is_some_and(|v| v.to_str().is_some_and(|v| v.trim().is_empty()));
    if blank { arg.env(None::<&str>) } else { arg }
}

/// clap names the flag when a value from its variable is bad or conflicts with another, as if the
/// flag had been given: name the variable instead when the flag isn't among `args` (without the
/// binary name).
fn name_env_vars(mut error: clap::Error, cmd: &Command, args: &[OsString]) -> clap::Error {
    use clap::error::{ContextKind, ContextValue};
    // `--flag value` or `--flag=value`.
    let given =
        |long: &str| args.iter().filter_map(|a| a.to_str()?.strip_prefix("--")?.strip_prefix(long)).any(|r| r.is_empty() || r.starts_with('='));
    let sub = args.first().and_then(|name| cmd.find_subcommand(name)).unwrap_or(cmd);
    for kind in [ContextKind::InvalidArg, ContextKind::PriorArg] {
        let Some(ContextValue::String(shown)) = error.get(kind) else { continue };
        let Some(arg) = sub.get_arguments().find(|a| a.to_string() == *shown) else { continue };
        let (Some(long), Some(var)) = (arg.get_long(), arg.get_env()) else { continue };
        if !given(long) {
            error.insert(kind, ContextValue::String(var.to_string_lossy().into_owned()));
        }
    }
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    /// Parse without the variables, so one exported where the tests run can't change them.
    /// `variable_cases` covers the variables in child processes.
    fn parse_args(args: &[&str]) -> Result<LaunchArgs, clap::Error> {
        parse(command().mut_args(|a| a.env(None::<&str>)), args.iter().map(OsString::from))
    }

    fn usage_error(args: &[&str]) -> String {
        let Err(e) = parse_args(args) else { panic!("{args:?} parsed") };
        assert_eq!(e.exit_code(), 2, "{args:?}: {e}");
        e.to_string()
    }

    #[test]
    fn values_are_parsed() {
        let parsed = parse_args(&["--control", " 50494 ", "--sample", "a.dxf", "b.dwg"]).unwrap();
        assert_eq!((parsed.control, parsed.sample), (Some(50494), true));
        assert_eq!(parsed.file_names(), FileNames { openable: vec!["a.dxf".into(), "b.dwg".into()], unreadable: vec![] });
        let parsed = parse_args(&[]).unwrap();
        assert_eq!((parsed.control, parsed.sample, parsed.files.len()), (None, false, 0));
    }

    #[test]
    fn control_port_rejects_unparseable_values_naming_them() {
        for bad in ["78787", "-1", "abc", "99999999999999999999"] {
            assert!(usage_error(&["--control", bad]).contains(&format!("invalid value '{bad}' for '--control <PORT>'")), "{bad}");
        }
        assert!(usage_error(&["--control="]).contains("invalid value '' for '--control <PORT>'"));
    }

    #[test]
    fn usage_errors_exit_2() {
        assert!(usage_error(&["--sampel"]).contains("unexpected argument '--sampel'"));
        assert!(usage_error(&["--control"]).contains("a value is required for '--control <PORT>'"));
        // A following flag is never taken as the value.
        assert!(usage_error(&["--control", "--sample"]).contains("a value is required for '--control <PORT>'"));
        assert!(usage_error(&["--sample", "--sample"]).contains("cannot be used multiple times"));
        assert!(usage_error(&["--control", "1", "--control", "2"]).contains("cannot be used multiple times"));
    }

    #[test]
    fn version_stops_parsing() {
        let e = parse_args(&["-V", "--control"]).err().unwrap();
        assert_eq!((e.kind(), e.exit_code()), (ErrorKind::DisplayVersion, 0));
        assert_eq!(e.to_string(), format!("{}\n", crate::version_string()));
        let e = parse_args(&["--version", "--bogus"]).err().unwrap();
        assert_eq!(e.kind(), ErrorKind::DisplayVersion);
    }

    #[test]
    fn help_stops_parsing_and_double_dash_ends_options() {
        let e = parse_args(&["-h", "--sampel", "--control"]).err().unwrap();
        assert_eq!((e.kind(), e.exit_code()), (ErrorKind::DisplayHelp, 0));
        let help = parse_args(&["--help"]).err().unwrap().to_string();
        assert!(help.contains("--control <PORT>") && help.contains("CADCRAFT_CONFIG_DIR"), "{help}");

        assert!(usage_error(&["--sampel", "--", "--help"]).contains("unexpected argument '--sampel'"));
        let parsed = parse_args(&["a.dxf", "--", "--help", "-x"]).unwrap();
        assert_eq!(parsed.file_names().openable, ["a.dxf", "--help", "-x"]);
    }

    #[test]
    fn a_finder_process_serial_number_is_dropped() {
        let parsed = parse_args(&["-psn_0_12345", "a.dxf"]).unwrap();
        assert_eq!(parsed.file_names().openable, ["a.dxf"]);
        let parsed = parse_args(&["--", "-psn_x.dxf"]).unwrap();
        assert_eq!(parsed.file_names().openable, ["-psn_x.dxf"]);
    }

    /// A name that isn't valid Unicode is reported, not opened.
    #[cfg(unix)]
    #[test]
    fn non_unicode_file_names_are_reported() {
        use std::os::unix::ffi::OsStringExt as _;
        let latin1 = OsString::from_vec(b"caf\xe9.dxf".to_vec());
        let parsed = parse(command().mut_args(|a| a.env(None::<&str>)), [latin1, OsString::from("b.dxf")]).unwrap();
        assert_eq!(parsed.file_names(), FileNames { openable: vec!["b.dxf".into()], unreadable: vec!["caf\u{fffd}.dxf".into()] });
    }

    /// The variable cases run in child processes (`variable_child`), since the crate forbids
    /// `unsafe` and so `set_var`.
    #[test]
    fn variable_cases() {
        let run = |case: &str, value: &str| {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "launch_args::tests::variable_child", "--ignored", "--nocapture", "--test-threads=1"])
                .env("CADCRAFT_CONTROL_PORT", value)
                .env("LAUNCH_ARGS_CASE", case)
                .output()
                .unwrap();
            assert!(out.status.success(), "{case}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        };
        run("used", " 50494 ");
        run("flag_wins", "not-a-port");
        run("blank_is_unset", "  ");
        run("empty_is_unset", "");
        run("bad_names_variable", "78787");
    }

    #[test]
    #[ignore = "run by variable_cases with CADCRAFT_CONTROL_PORT set"]
    fn variable_child() {
        let args = |a: &[&str]| parse(command(), a.iter().map(OsString::from));
        match std::env::var("LAUNCH_ARGS_CASE").unwrap().as_str() {
            "used" => assert_eq!(args(&[]).unwrap().control, Some(50494)),
            "flag_wins" => assert_eq!(args(&["--control", "7"]).unwrap().control, Some(7)),
            "blank_is_unset" | "empty_is_unset" => assert_eq!(args(&[]).unwrap().control, None),
            "bad_names_variable" => {
                let e = args(&[]).err().unwrap();
                assert_eq!(e.exit_code(), 2);
                let text = e.to_string();
                assert!(text.contains("invalid value '78787' for 'CADCRAFT_CONTROL_PORT'"), "{text}");
                assert!(!text.contains("--control <PORT>'"), "{text}");
            }
            other => panic!("unknown case {other}"),
        }
    }
}
