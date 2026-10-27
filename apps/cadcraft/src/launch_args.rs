//! The desktop app's command line: `cadcraft [--control <port>] [--sample] [--version] [files…]`.

/// The control port from `--control <port>` or `CADCRAFT_CONTROL_PORT` (`source`); a value that is
/// not a port is logged and ignored instead of silently starting no control channel.
fn control_port_from(source: &str, value: Option<String>) -> Option<u16> {
    let value = value?;
    let port = value.trim().parse().ok();
    if port.is_none() {
        log::warn!("{source}: {value:?} is not a port number; the control channel is off");
    }
    port
}

/// `--control` followed by `value`: a bare `--control` (last argument) keeps the port already
/// read from `CADCRAFT_CONTROL_PORT` (`current`) instead of turning the control channel off.
fn control_flag(current: Option<u16>, value: Option<String>) -> Option<u16> {
    if value.is_some() {
        return control_port_from("--control", value);
    }
    match current {
        Some(port) => log::warn!("--control: no port given; using CADCRAFT_CONTROL_PORT ({port})"),
        None => log::warn!("--control: no port given; the control channel is off"),
    }
    current
}

/// What the command line asks for.
pub(crate) struct LaunchArgs {
    pub(crate) control_port: Option<u16>,
    pub(crate) sample: bool,
    pub(crate) files: Vec<String>,
    /// `--version` / `-V`: print the version and exit; the rest of the arguments were not read.
    pub(crate) version: bool,
}

/// Parse the arguments (without the binary name) and `CADCRAFT_CONTROL_PORT`.
pub(crate) fn parse(mut args: impl Iterator<Item = String>) -> LaunchArgs {
    // lossy, so a value that is not Unicode is reported like any other bad port instead of ignored
    let mut control_port =
        control_port_from("CADCRAFT_CONTROL_PORT", std::env::var_os("CADCRAFT_CONTROL_PORT").map(|p| p.to_string_lossy().into_owned()));
    let mut files = Vec::new();
    let mut sample = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--control" => control_port = control_flag(control_port, args.next()),
            "--sample" => sample = true,
            "--version" | "-V" => return LaunchArgs { control_port, sample, files, version: true },
            _ => files.push(a),
        }
    }
    LaunchArgs { control_port, sample, files, version: false }
}

#[cfg(test)]
mod tests {
    use super::{control_flag, control_port_from};

    #[test]
    fn control_port_values_that_are_not_ports_are_ignored() {
        assert_eq!(control_port_from("--control", Some("7979".into())), Some(7979));
        assert_eq!(control_port_from("--control", Some(" 7979 ".into())), Some(7979));
        assert_eq!(control_port_from("--control", None), None);
        for bad in ["", "abc", "-1", "65536", "99999999999999999999", "\u{0}"] {
            assert_eq!(control_port_from("--control", Some(bad.into())), None, "{bad:?}");
        }
    }

    #[test]
    fn control_without_a_port_keeps_the_environment_port() {
        // `CADCRAFT_CONTROL_PORT=47321 cadcraft --control` (#844)
        assert_eq!(control_flag(Some(47321), None), Some(47321));
        assert_eq!(control_flag(None, None), None);
        // A port given on the command line still wins over the variable.
        assert_eq!(control_flag(Some(47321), Some("7979".into())), Some(7979));
        assert_eq!(control_flag(Some(47321), Some("abc".into())), None);
    }
}
