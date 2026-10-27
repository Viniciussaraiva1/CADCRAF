//! `cadcraft-cli`: headless CADCraft.
//!
//! ```text
//! cadcraft-cli info FILE.dxf                       summary as JSON
//! cadcraft-cli convert IN.dxf OUT.(dxf|svg|png) [--window x0,y0,x1,y1 | --view extents|current]
//! cadcraft-cli run [FILE|--sample|--metric] [--script TEXT|--script-file F.scr] [--cmd 'id {json}']... [--save OUT] [--export OUT.png] [--window ... | --view ...]
//! cadcraft-cli commands [FILTER]                   command catalog (JSON)
//! cadcraft-cli mcp [--connect HOST:PORT [--lazy] [--launch]]  MCP server on stdio (headless or bridged to the app)
//! cadcraft-cli perf [N]                            timing table on a synthetic N-entity drawing
//! cadcraft-cli perf FILE                           canvas display-list build of a drawing, zoomed in step by step
//! cadcraft-cli sample (bracket|floorplan) OUT       write a built-in sample drawing
//! ```
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::ffi::OsString;
use std::io::Write;

use cadcraft_engine::doc::{Handle, Space};
use cadcraft_engine::geom::{Bounds2, Vec2};
use cadcraft_engine::{Session, View};
use serde_json::{Value, json};

mod args;
mod convert;
mod error;
mod info;
mod mcp;
mod output;
mod perf;
mod run;
mod sample;

#[cfg(test)]
use args::{Framing, check_framed_output, framing_option, parse_window};
use args::{NO_OPTIONS, USAGE, validate_options};
use convert::convert;
use error::{CliError, Result};
use info::info;
use mcp::mcp;
use output::Streams;
#[cfg(test)]
use output::{export_window, model_views, write_atomic};
use perf::perf;
use run::run_cmds;
#[cfg(test)]
use run::{fill_last, returned_handle};
use sample::sample;

fn install_io() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: Some(|d, space, opts| cadcraft_io::plotter::plot(d, space, opts).map_err(|e| e.to_string())),
    });
    cadcraft_engine::cmd::file::set_write_as(|d, name, o| {
        cadcraft_io::write_with(d, name, &cadcraft_io::SaveOptions { version: o.version, binary: o.binary }).map_err(|e| e.to_string())
    });
    // AutoLISP's entget, entmake and entmod see entities as DXF groups.
    cadcraft_engine::cmd::file::set_entity_codec(cadcraft_engine::cmd::file::EntityCodec {
        groups: cadcraft_io::entity_groups,
        entity: cadcraft_io::entity_from_groups,
    });
}

fn open(s: &mut Session, path: &str) -> Result {
    s.execute("open", &json!({ "path": path }))?;
    Ok(())
}

/// Run the CLI on its arguments (without the program name). Returns the process exit code (0 ok,
/// 1 failure). A non-Unicode argument is an error (exit 1), where `std::env::args()` would panic.
pub fn run<I, T>(args: I, out: &mut dyn Write, err: &mut dyn Write) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    // The engine's hooks are set-once, so installing them on every call is harmless.
    install_io();
    let mut io = Streams::new(out, err);
    let result = args
        .into_iter()
        .map(|a| a.into().into_string().map_err(CliError::NonUnicodeArg))
        .collect::<Result<Vec<String>>>()
        .and_then(|args| dispatch(&args, &mut io));
    let mut code = 0;
    if let Err(e) = result {
        if e.is_usage() {
            io.errln(format_args!("{e}"));
        } else {
            io.errln(format_args!("error: {e}"));
        }
        code = 1;
    }
    io.finish();
    if io.stdout_failed() { 1 } else { code }
}

fn dispatch(args: &[String], io: &mut Streams) -> Result {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("info") => {
            validate_options("info", rest, NO_OPTIONS)?;
            info(rest.first().ok_or_else(|| CliError::Usage(USAGE.into()))?, io)
        }
        Some("convert") => convert(rest, io),
        Some("run") => run_cmds(rest, io),
        Some("perf") => {
            validate_options("perf", rest, NO_OPTIONS)?;
            perf(rest, io)
        }
        Some("sample") => {
            validate_options("sample", rest, NO_OPTIONS)?;
            sample(rest, io)
        }
        Some("commands") => {
            validate_options("commands", rest, NO_OPTIONS)?;
            let s = Session::new();
            let f = rest.first().map(|x| x.to_ascii_lowercase());
            let v: Vec<Value> = cadcraft_engine::command_specs()
                .iter()
                .filter(|c| f.as_ref().is_none_or(|f| c.id.contains(f.as_str()) || c.label.to_ascii_lowercase().contains(f.as_str())))
                .map(|c| serde_json::to_value(c.info(&s)).unwrap_or_default())
                .collect();
            io.outln(format_args!("{}", serde_json::to_string_pretty(&v).unwrap_or_default()));
            Ok(())
        }
        Some("mcp") => mcp(rest),
        Some("--version" | "-V") => {
            io.outln(format_args!("cadcraft-cli {}", env!("CARGO_PKG_VERSION")));
            Ok(())
        }
        Some("--help" | "-h" | "help") => {
            io.outln(format_args!("{USAGE}"));
            Ok(())
        }
        _ => Err(CliError::Usage(USAGE.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cadcraft-cli-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn write_atomic_replaces_file_and_leaves_no_staging_files() {
        let dir = scratch_dir("replace");
        let out = dir.join("a.dxf");
        let out = out.to_str().expect("utf-8 path");
        write_atomic(out, b"old").expect("create");
        write_atomic(out, b"new").expect("overwrite");
        assert_eq!(std::fs::read(out).expect("read"), b"new");
        assert_eq!(std::fs::read_dir(&dir).expect("list").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_failure_keeps_destination_and_cleans_up() {
        let dir = scratch_dir("fail");
        let out = dir.join("target");
        std::fs::create_dir(&out).expect("dir as destination");
        std::fs::write(out.join("keep"), b"x").expect("seed");
        assert!(write_atomic(out.to_str().expect("utf-8 path"), b"new").is_err());
        assert_eq!(std::fs::read(out.join("keep")).expect("kept"), b"x");
        assert_eq!(std::fs::read_dir(&dir).expect("list").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn b(x0: f64, y0: f64, x1: f64, y1: f64) -> Bounds2 {
        Bounds2::new(Vec2::new(x0, y0), Vec2::new(x1, y1))
    }

    fn sample_session() -> Session {
        let mut s = Session::empty();
        s.open_drawing(cadcraft_engine::sample::default_sample(), "Bracket", None);
        s
    }

    fn view_box(svg: &str) -> String {
        let i = svg.find("viewBox=\"").map(|i| i + 9).unwrap_or(0);
        svg[i..].split('"').next().unwrap_or("").to_string()
    }

    fn assert_near(a: Bounds2, e: Bounds2) {
        let d = (a.min - e.min).len().max((a.max - e.max).len());
        assert!(d < 1e-9, "{a:?} != {e:?}");
    }

    #[test]
    fn window_parses() {
        assert_eq!(parse_window("0,0,30,20").unwrap(), b(0.0, 0.0, 30.0, 20.0));
        assert_eq!(parse_window(" -5.5, -1e3 ,2e3,4 ").unwrap(), b(-5.5, -1000.0, 2000.0, 4.0));
    }

    #[test]
    fn hostile_window_is_an_error() {
        for w in [
            "",
            "1,2,3",
            "1,2,3,4,5",
            "a,0,1,1",
            "NaN,0,1,1",
            "0,0,inf,1",
            "0,0,1e400,1",
            "0,0,0,1",
            "0,0,1,0",
            "5,0,1,1",
            "0,5,1,1",
            "-1e300,-1e300,1e300,1e300",
            "0,0,1e-12,1e-12",
        ] {
            assert!(parse_window(w).is_err(), "`{w}` should be rejected");
        }
    }

    #[test]
    fn framing_options() {
        let parse = |args: &[&str]| -> Result<Framing> {
            let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            let mut f = Framing::Auto;
            let mut it = args.iter();
            while let Some(a) = it.next() {
                assert!(framing_option(a, &mut it, &mut f)?, "{a} is a framing option");
            }
            Ok(f)
        };
        assert_eq!(parse(&["--view", "extents"]).unwrap(), Framing::Extents);
        assert_eq!(parse(&["--view", "CURRENT"]).unwrap(), Framing::Current);
        assert_eq!(parse(&["--window", "0,0,2,1"]).unwrap(), Framing::Window(b(0.0, 0.0, 2.0, 1.0)));
        assert!(parse(&["--view"]).is_err());
        assert!(parse(&["--view", "selection"]).is_err());
        assert!(parse(&["--window"]).is_err());
        assert!(parse(&["--window", "0,0,2,1", "--view", "extents"]).is_err());
        assert!(check_framed_output(Framing::Extents, Some("a.dxf")).is_err());
        assert!(check_framed_output(Framing::Extents, None).is_err());
        assert!(check_framed_output(Framing::Extents, Some("a.png")).is_ok());
        assert!(check_framed_output(Framing::Auto, Some("a.dxf")).is_ok());
    }

    #[test]
    fn last_placeholder_takes_the_previous_handle() {
        let mut s = sample_session();
        let r = s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
        let h = returned_handle(&s, &r).expect("LINE returns its handle");
        assert_eq!(fill_last(&json!({"handle": "{last}"}), Some(h)).unwrap(), json!({"handle": h.hex()}));
        // Nested, as in a selection list.
        assert_eq!(fill_last(&json!({"handles": ["{last}", "1"]}), Some(h)).unwrap(), json!({"handles": [h.hex(), "1"]}));
        // Only the exact token: a name or a sentence that merely contains it stays as written.
        for p in [json!("last"), json!("{last} "), json!({"name": "last"})] {
            assert_eq!(fill_last(&p, Some(h)).unwrap(), p);
        }
        // Nothing returned yet: an error, not a token passed to the command as a bad handle.
        assert!(fill_last(&json!({"handle": "{last}"}), None).unwrap_err().to_string().contains("{last}"));
    }

    /// Handles are read from a result by looking the strings up in the drawing: counts and names
    /// are not mistaken for handles, and `mview`'s `viewports` works like `handle`/`handles`.
    #[test]
    fn returned_handle_reads_only_real_handles() {
        let mut s = Session::empty();
        s.new_drawing(false);
        s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
        let r = s.execute("mview", &json!({"p1": [20, 20], "p2": [200, 150]})).unwrap();
        let h = returned_handle(&s, &r).expect("mview returns its viewport handle");
        assert!(r["viewports"].as_array().is_some_and(|a| a.iter().any(|v| v.as_str() == Some(h.hex().as_str()))), "{r}");
        assert_eq!(returned_handle(&s, &json!({"changed": 3, "name": "A1"})), None);
        // The most recent of several.
        let a = s.execute("line", &json!({"points": [[0, 0], [1, 0]]})).unwrap();
        let b = s.execute("line", &json!({"points": [[0, 0], [2, 0]]})).unwrap();
        let (ha, hb) = (returned_handle(&s, &a).unwrap(), returned_handle(&s, &b).unwrap());
        assert_eq!(returned_handle(&s, &json!({"handles": [ha.hex(), hb.hex()]})), Some(hb));
    }

    #[test]
    fn default_framing_is_extents() {
        // convert: nothing ran, always the extents.
        let s = sample_session();
        assert_eq!(export_window(&s, Framing::Auto, None).unwrap(), None);
        // run: commands that leave the view alone keep the extents framing.
        let mut s = sample_session();
        let before = model_views(&s);
        s.script("LINE 0,0 10,10\n\n").unwrap();
        assert_eq!(export_window(&s, Framing::Auto, Some(&before)).unwrap(), None);
        // A fresh drawing too.
        let mut s = Session::empty();
        s.new_drawing(false);
        let before = model_views(&s);
        s.execute("circle", &json!({"center": [0, 0], "radius": 5})).unwrap();
        assert_eq!(export_window(&s, Framing::Auto, Some(&before)).unwrap(), None);
    }

    #[test]
    fn zoom_window_frames_export() {
        let mut s = sample_session();
        let before = model_views(&s);
        s.execute("zoom", &json!({"mode": "window", "p1": [0, 0], "p2": [30, 20]})).unwrap();
        let w = export_window(&s, Framing::Auto, Some(&before)).unwrap().unwrap();
        // The CLI viewport is 3:2, so a 3:2 window is shown exactly.
        assert_near(w, b(0.0, 0.0, 30.0, 20.0));
        assert_eq!(export_window(&s, Framing::Current, Some(&before)).unwrap(), Some(w));
        // --view extents and --window override the zoom.
        assert_eq!(export_window(&s, Framing::Extents, Some(&before)).unwrap(), None);
        let explicit = b(1.0, 1.0, 2.0, 2.0);
        assert_eq!(export_window(&s, Framing::Window(explicit), Some(&before)).unwrap(), Some(explicit));
        // A taller window keeps its height; the 3:2 image adds room left and right.
        s.execute("zoom", &json!({"mode": "center", "center": [5, 5], "height": 4})).unwrap();
        assert_near(export_window(&s, Framing::Auto, Some(&before)).unwrap().unwrap(), b(2.0, 3.0, 8.0, 7.0));
    }

    #[test]
    fn zoom_object_frames_the_selection() {
        let mut s = sample_session();
        let before = model_views(&s);
        // The smallest entity with an area, selected then framed with ZOOM Object.
        let d = s.doc().unwrap();
        let bounds = |e: &cadcraft_engine::doc::Entity| cadcraft_engine::doc::entity_bounds(d, e, 0);
        let e = d
            .model
            .iter()
            .filter(|e| bounds(e).width() > 0.0 && bounds(e).height() > 0.0)
            .min_by(|a, b| bounds(a).width().total_cmp(&bounds(b).width()))
            .unwrap();
        let (h, eb, ext) = (e.handle, bounds(e), d.extents(&Space::Model));
        s.set_selection(vec![h]);
        s.execute("zoom", &json!({"mode": "object"})).unwrap();
        let w = export_window(&s, Framing::Auto, Some(&before)).unwrap().unwrap();
        assert!(w.contains(eb.min) && w.contains(eb.max));
        assert!(w.width() < ext.width() && w.height() < ext.height(), "{w:?} vs extents {ext:?}");
    }

    #[test]
    fn run_export_honours_zoom() {
        let dir = std::env::temp_dir().join(format!("cadcraft-cli-165-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = |n: &str| dir.join(n).to_string_lossy().to_string();
        let run_svg = |extra: &[&str], out: &str| -> String {
            let mut args: Vec<String> = ["--sample"].iter().chain(extra).map(|a| a.to_string()).collect();
            args.extend(["--export".to_string(), path(out)]);
            run_cmds(&args, &mut Streams::new(&mut Vec::new(), &mut Vec::new())).unwrap();
            std::fs::read_to_string(path(out)).unwrap()
        };
        let base = run_svg(&[], "base.svg");
        let zoomed = run_svg(&["--cmd", r#"zoom {"mode":"window","p1":[0,0],"p2":[30,20]}"#], "zoom.svg");
        let window = run_svg(&["--window", "0,0,30,20"], "window.svg");
        let forced = run_svg(&["--cmd", r#"zoom {"mode":"window","p1":[0,0],"p2":[30,20]}"#, "--view", "extents"], "forced.svg");
        assert_ne!(view_box(&zoomed), view_box(&base));
        assert_eq!(view_box(&zoomed), "0 0 30.000000 20.000000");
        assert_eq!(zoomed, window);
        assert_eq!(forced, base);
        // Hostile --window: a clear error, nothing written.
        let bad: Vec<String> = ["--sample", "--window", "0,0,NaN,1", "--export", &path("bad.png")].iter().map(|a| a.to_string()).collect();
        assert!(run_cmds(&bad, &mut Streams::new(&mut Vec::new(), &mut Vec::new())).unwrap_err().to_string().contains("--window"));
        assert!(!dir.join("bad.png").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn validators_accept_only_the_documented_options() {
        assert!(validate_options("info", &args(&["file.dxf"]), NO_OPTIONS).is_ok());
        assert!(validate_options("perf", &args(&["100"]), NO_OPTIONS).is_ok());
        assert!(validate_options("sample", &args(&["bracket", "out.dxf"]), NO_OPTIONS).is_ok());
        assert!(validate_options("commands", &args(&["line"]), NO_OPTIONS).is_ok());
        for (command, specs) in [("info", NO_OPTIONS), ("perf", NO_OPTIONS), ("sample", NO_OPTIONS), ("commands", NO_OPTIONS)] {
            assert!(validate_options(command, &args(&["--typo"]), specs).is_err());
        }
    }

    #[test]
    fn unknown_options_are_rejected_before_side_effects() {
        assert_eq!(validate_options("info", &args(&["missing.dxf", "--typo"]), NO_OPTIONS).unwrap_err().to_string(), "info: unknown option --typo");
        assert_eq!(validate_options("perf", &args(&["1", "--typo"]), NO_OPTIONS).unwrap_err().to_string(), "perf: unknown option --typo");
        assert_eq!(
            validate_options("sample", &args(&["bracket", "out.dxf", "--typo"]), NO_OPTIONS).unwrap_err().to_string(),
            "sample: unknown option --typo"
        );
    }
}
