use super::*;

pub(crate) const USAGE: &str = "usage:
  cadcraft-cli info FILE.dxf
  cadcraft-cli convert IN.dxf OUT.(dxf|dwg|svg|png|pdf) [FRAMING]
  cadcraft-cli run [FILE | --sample | --metric] [--script TEXT] [--script-file F.scr] [--cmd 'id {json}']... [--save OUT.dxf] [--export OUT.(png|svg|pdf)] [FRAMING]
  cadcraft-cli commands [FILTER]
  cadcraft-cli mcp [--connect HOST:PORT [--lazy] [--launch]]
  cadcraft-cli perf [N | FILE]
  cadcraft-cli sample (bracket|floorplan) OUT.(dxf|dwg|svg|png)
  cadcraft-cli --version

image framing (png 2400x1600, svg, pdf; always model space):
  default                  convert: fit the drawing extents.
                           run --export: the current view if the script/commands changed it
                           (zoom, pan, view.set...), otherwise fit the drawing extents.
  --window x0,y0,x1,y1     show this rectangle (drawing units, x0<x1, y0<y1), centred and
                           scaled to fit the image without distortion.
  --view extents           always fit the drawing extents.
  --view current           the current view (as last set by zoom/pan/view.set; a freshly
                           opened drawing's view is its extents plus a 10% margin).

--cmd chaining (run):
  A --cmd parameter value of exactly \"{last}\" is replaced with the last object handle an
  earlier --cmd in the same run returned, so a command can act on what the previous one made:
    --cmd 'mview {\"p1\":[20,20],\"p2\":[200,150]}'        prints {\"viewports\":[\"100\"]}
    --cmd 'viewport.set {\"handle\":\"{last}\",\"scale\":1}' changes viewport 100
  It works in any parameter, at any depth (e.g. \"handles\":[\"{last}\"]); every other string is
  passed unchanged. With no earlier returned handle the run fails and says so.";

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

/// Why the arguments after a subcommand are rejected.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ArgsError {
    #[error("--window needs x0,y0,x1,y1")]
    WindowNeedsValue,
    #[error("--view needs `extents` or `current`")]
    ViewNeedsValue,
    #[error("use only one of --window and --view")]
    FramingConflict,
    #[error("--window/--view only apply to image output (png, svg, pdf), not `{0}`")]
    FramingNeedsImage(String),
    #[error("--window/--view need an image output (--export OUT.png|svg|pdf)")]
    FramingNeedsExport,
    #[error("--connect needs HOST:PORT (the app's --control port, e.g. 127.0.0.1:7979)")]
    ConnectNeedsAddr,
    #[error("--connect given more than once")]
    ConnectTwice,
    #[error("--lazy and --launch need --connect HOST:PORT")]
    LazyNeedsConnect,
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

/// Parse a framing option (`--window`, `--view`) at `a`, taking its value from `it`. `Ok(false)`
/// means `a` is not a framing option.
pub(crate) fn framing_option<'a>(a: &str, it: &mut impl Iterator<Item = &'a String>, framing: &mut Framing) -> Result<bool> {
    let f = match a {
        "--window" => Framing::Window(parse_window(it.next().ok_or(ArgsError::WindowNeedsValue)?)?),
        "--view" => match it.next().map(|v| v.to_ascii_lowercase()).as_deref() {
            Some("extents") => Framing::Extents,
            Some("current") => Framing::Current,
            _ => return Err(ArgsError::ViewNeedsValue.into()),
        },
        _ => return Ok(false),
    };
    if *framing != Framing::Auto && std::mem::discriminant(framing) != std::mem::discriminant(&f) {
        return Err(ArgsError::FramingConflict.into());
    }
    *framing = f;
    Ok(true)
}

#[derive(Clone, Copy)]
pub(crate) struct OptionSpec {
    pub(crate) name: &'static str,
    takes_value: bool,
}

pub(crate) fn validate_options(command: &str, args: &[String], specs: &[OptionSpec]) -> Result {
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        if let Some(name) = arg.strip_prefix("--") {
            let Some(spec) = specs.iter().find(|spec| spec.name == arg) else {
                return Err(CliError::Usage(format!("{command}: unknown option --{name}")));
            };
            if spec.takes_value && args.get(i + 1).is_some_and(|value| !value.starts_with("--")) {
                i += 2;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    Ok(())
}

pub(crate) const NO_OPTIONS: &[OptionSpec] = &[];

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

/// Parses the arguments after `mcp`. `--connect` must be followed by a `HOST:PORT`; a missing,
/// empty or flag-like value is an error, never a silent fall back to a headless session (#103).
pub(crate) fn parse_mcp_args(args: &[String]) -> Result<McpTarget> {
    let mut addr: Option<String> = None;
    let (mut lazy, mut launch) = (false, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--connect" => {
                let a = it.next().map(|v| v.trim()).filter(|v| !v.is_empty() && !v.starts_with('-')).ok_or(ArgsError::ConnectNeedsAddr)?;
                if addr.is_some() {
                    return Err(ArgsError::ConnectTwice.into());
                }
                addr = Some(a.to_string());
            }
            "--lazy" => lazy = true,
            "--launch" => launch = true,
            other => return Err(CliError::Usage(format!("mcp: unknown argument {other}\n{USAGE}"))),
        }
    }
    match addr {
        // `--launch` connects at the first call too (the app opens then).
        Some(addr) => Ok(McpTarget::Connect { addr, lazy: lazy || launch, launch }),
        None if lazy || launch => Err(ArgsError::LazyNeedsConnect.into()),
        None => Ok(McpTarget::Headless),
    }
}

#[cfg(test)]
mod mcp_args_tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn mcp_without_connect_is_headless() {
        assert_eq!(parse_mcp_args(&args(&[])).unwrap(), McpTarget::Headless);
    }

    #[test]
    fn mcp_connect_with_address_selects_remote() {
        let to = |addr: &str, lazy, launch| McpTarget::Connect { addr: addr.into(), lazy, launch };
        assert_eq!(parse_mcp_args(&args(&["--connect", "127.0.0.1:7979"])).unwrap(), to("127.0.0.1:7979", false, false));
        assert_eq!(parse_mcp_args(&args(&["--connect", "localhost:7979"])).unwrap(), to("localhost:7979", false, false));
        assert_eq!(parse_mcp_args(&args(&["--lazy", "--connect", "localhost:7979"])).unwrap(), to("localhost:7979", true, false));
        assert_eq!(parse_mcp_args(&args(&["--connect", "localhost:7979", "--launch"])).unwrap(), to("localhost:7979", true, true));
        // Without an app to reach they mean nothing: never a headless session.
        assert!(parse_mcp_args(&args(&["--lazy"])).is_err());
        assert!(parse_mcp_args(&args(&["--launch"])).is_err());
    }

    #[test]
    fn mcp_connect_without_address_is_an_error_not_headless() {
        for bad in [&["--connect"][..], &["--connect", ""], &["--connect", "  "], &["--connect", "--foo"], &["--connect", "-v"]] {
            let r = parse_mcp_args(&args(bad));
            let Err(e) = r else { panic!("{bad:?} should be rejected, got {r:?}") };
            assert!(e.to_string().contains("--connect needs HOST:PORT"), "{bad:?}: {e}");
        }
    }

    #[test]
    fn mcp_rejects_unknown_and_repeated_arguments() {
        assert!(parse_mcp_args(&args(&["--conect", "127.0.0.1:7979"])).is_err());
        assert!(parse_mcp_args(&args(&["127.0.0.1:7979"])).is_err());
        assert!(parse_mcp_args(&args(&["--connect", "127.0.0.1:1", "--connect", "127.0.0.1:2"])).is_err());
    }
}
