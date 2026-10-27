use super::args::{Framing, check_framed_output, framing_option};
use super::output::{Streams, export, export_framed, export_window, model_views};
use super::*;

/// A `--cmd` parameter value of exactly `{last}` is filled in with the last object handle an
/// earlier `--cmd` in the same run returned: `mview` answers `{"viewports":["100"]}`, so
/// `viewport.set {"handle":"{last}","scale":1}` changes viewport 100. Any other string is passed
/// through unchanged, and `--script` text is never rewritten.
const LAST_HANDLE: &str = "{last}";

/// Why a `run` argument or step is rejected.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RunError {
    #[error("{option} needs {what}")]
    MissingValue { option: &'static str, what: &'static str },
    #[error("{path}: {source}")]
    ReadScript { path: String, source: std::io::Error },
    #[error("--cmd {id}: {source}")]
    CmdJson { id: String, source: serde_json::Error },
    #[error("--cmd {id}: {source}")]
    CmdLast { id: String, source: NoLastHandle },
}

/// A `{last}` placeholder with no handle to fill in.
#[derive(Debug, PartialEq, thiserror::Error)]
#[error("`{{last}}` needs an earlier --cmd in this run to have returned a handle")]
pub(crate) struct NoLastHandle;

/// How deep [`result_handles`] walks a command result (results are the engine's own JSON).
const RESULT_DEPTH: usize = 16;

/// Result keys whose values are object references: the engine's result convention for handles.
/// A handle can be a string (`handle`, `viewport`, `insert`…) or a list of them (`handles`,
/// `viewports`, `members`…). Every other field may hold a user-defined name, a message or a
/// count, and is never read as a handle — a group named `100` is a name, not object `100`.
fn handle_field(key: &str) -> bool {
    matches!(
        key,
        "handle"
            | "handles"
            | "viewport"
            | "viewports"
            | "boundary"
            | "clip"
            | "insert"
            | "members"
            | "source"
            | "objects"
            | "entities"
            | "selection"
            | "parts"
            | "refs"
            | "matches"
            | "line"
            | "arc"
            | "blocks"
    )
}

/// Collect the handles of `v`, a value under a handle field: a handle string, an array of them,
/// or objects that carry handle fields of their own (`entities`, `parts`).
fn collect_result_handles(s: &Session, v: &Value, depth: usize, out: &mut Vec<Handle>) {
    if depth > RESULT_DEPTH {
        return;
    }
    match v {
        Value::String(t) => {
            if let Some(h) = Handle::parse_hex(t)
                && s.doc().is_ok_and(|d| d.entity(h).is_some())
            {
                out.push(h);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_result_handles(s, x, depth + 1, out)),
        Value::Object(m) => {
            for (k, x) in m {
                if handle_field(k) {
                    collect_result_handles(s, x, depth + 1, out);
                } else {
                    find_result_handles(s, x, depth + 1, out);
                }
            }
        }
        _ => {}
    }
}

/// Find handle fields nested in containers that are not handles themselves (`groups` in
/// `groups.list`, whose members are handles): strings here are names, never references.
fn find_result_handles(s: &Session, v: &Value, depth: usize, out: &mut Vec<Handle>) {
    if depth > RESULT_DEPTH {
        return;
    }
    match v {
        Value::Array(a) => a.iter().for_each(|x| find_result_handles(s, x, depth + 1, out)),
        Value::Object(m) => {
            for (k, x) in m {
                if handle_field(k) {
                    collect_result_handles(s, x, depth + 1, out);
                } else {
                    find_result_handles(s, x, depth + 1, out);
                }
            }
        }
        _ => {}
    }
}

/// The object handles a command result mentions, in the order it lists them. A handle-only result
/// (a handle string or an array of them) counts as it is; otherwise only handle fields do.
fn result_handles(s: &Session, v: &Value, depth: usize, out: &mut Vec<Handle>) {
    match v {
        Value::String(_) => collect_result_handles(s, v, depth, out),
        Value::Array(a) if a.iter().all(Value::is_string) => collect_result_handles(s, v, depth, out),
        Value::Array(_) | Value::Object(_) => find_result_handles(s, v, depth, out),
        _ => {}
    }
}

/// The last handle `r` returned: what a later `{last}` stands for.
pub(crate) fn returned_handle(s: &Session, r: &Value) -> Option<Handle> {
    let mut hs = Vec::new();
    result_handles(s, r, 0, &mut hs);
    hs.last().copied()
}

/// The active drawing's identity ([`cadcraft_engine::DocState::uid`]): a handle is only meaningful
/// in the drawing it came from, and `new`, `open`, `document.switch` and `close` can make another
/// drawing current.
fn active_drawing(s: &Session) -> Option<u64> {
    s.docs.get(s.active).map(|st| st.uid)
}

/// Every `{last}` in `p`, at any depth, replaced by `h`. With no handle to fill in it is an
/// error, never a token reaching the command as a (bad) handle.
pub(crate) fn fill_last(p: &Value, h: Option<Handle>) -> Result<Value, NoLastHandle> {
    match p {
        Value::String(t) if t.as_str() == LAST_HANDLE => match h {
            Some(h) => Ok(Value::String(h.hex())),
            None => Err(NoLastHandle),
        },
        Value::Array(a) => a.iter().map(|x| fill_last(x, h)).collect::<Result<Vec<_>, _>>().map(Value::Array),
        Value::Object(m) => {
            m.iter().map(|(k, x)| Ok((k.clone(), fill_last(x, h)?))).collect::<Result<serde_json::Map<String, Value>, _>>().map(Value::Object)
        }
        other => Ok(other.clone()),
    }
}

pub(crate) fn run_cmds(args: &[String], io: &mut Streams) -> Result {
    let mut s = Session::empty();
    let mut it = args.iter();
    let mut save = None;
    let mut exp: Option<String> = None;
    let mut framing = Framing::Auto;
    let mut steps: Vec<(String, String)> = Vec::new();
    while let Some(a) = it.next() {
        if framing_option(a, &mut it, &mut framing)? {
            continue;
        }
        match a.as_str() {
            "--sample" => {
                s.open_drawing(cadcraft_engine::sample::default_sample(), "Bracket", None);
            }
            "--metric" => {
                s.new_drawing(true);
            }
            "--script" => {
                steps.push(("script".into(), it.next().ok_or(RunError::MissingValue { option: "--script", what: "text" })?.replace("\\n", "\n")))
            }
            "--script-file" => {
                let p = it.next().ok_or(RunError::MissingValue { option: "--script-file", what: "a path" })?;
                steps.push(("script".into(), std::fs::read_to_string(p).map_err(|source| RunError::ReadScript { path: p.clone(), source })?));
            }
            "--cmd" => steps.push(("cmd".into(), it.next().ok_or(RunError::MissingValue { option: "--cmd", what: "`id {json}`" })?.clone())),
            "--save" => save = Some(it.next().ok_or(RunError::MissingValue { option: "--save", what: "a path" })?.clone()),
            "--export" => exp = Some(it.next().ok_or(RunError::MissingValue { option: "--export", what: "a path" })?.clone()),
            f if !f.starts_with("--") => open(&mut s, f)?,
            other => return Err(CliError::Usage(format!("unknown option {other}"))),
        }
    }
    if s.docs.is_empty() {
        s.new_drawing(false);
    }
    check_framed_output(framing, exp.as_deref())?;
    let before = model_views(&s);
    let mut last_handle: Option<Handle> = None;
    for (kind, body) in steps {
        // A handle belongs to the drawing that returned it: if this step makes another drawing
        // current (`new`, `open`, `document.switch`, `close`, or a script running one of them),
        // the remembered handle is stale. A handle the step itself returns is taken afterwards,
        // so it is a handle of the drawing it made current.
        let drawing = active_drawing(&s);
        match kind.as_str() {
            "script" => {
                s.script(&body)?;
                if active_drawing(&s) != drawing {
                    last_handle = None;
                }
            }
            _ => {
                let (id, json) = body.split_once(' ').unwrap_or((&body, "{}"));
                let params: Value = serde_json::from_str(json).map_err(|source| RunError::CmdJson { id: id.to_string(), source })?;
                let params = fill_last(&params, last_handle).map_err(|source| RunError::CmdLast { id: id.to_string(), source })?;
                let r = s.execute(id, &params)?;
                if active_drawing(&s) != drawing {
                    last_handle = None;
                }
                if let Some(h) = returned_handle(&s, &r) {
                    last_handle = Some(h);
                }
                if !r.is_null() {
                    io.outln(format_args!("{r}"));
                }
            }
        }
    }
    for l in &s.log {
        io.errln(format_args!("{l}"));
    }
    if let Some(p) = save {
        export(&s, &p, io)?;
    }
    if let Some(p) = exp {
        let window = if cadcraft_io::is_image(&p) { export_window(&s, framing, Some(&before))? } else { None };
        export_framed(&s, &p, window, io)?;
    }
    Ok(())
}
