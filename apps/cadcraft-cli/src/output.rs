use super::args::Framing;
use super::*;

/// `println!` that doesn't panic when stdout is closed (`cadcraft-cli commands | head`) with
/// "failed printing to stdout: Broken pipe (os error 32)". Once stdout has failed, later output is
/// dropped and the work goes on: see `stdout_failed`.
macro_rules! outln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        if !$crate::output::stdout_gone() {
            if let Err(e) = writeln!(std::io::stdout(), $($arg)*) {
                $crate::output::stdout_failed(e);
            }
        }
    }};
}

/// Set once a write to stdout has failed; later output is dropped.
static STDOUT_GONE: AtomicBool = AtomicBool::new(false);

/// Set when stdout failed for a reason other than a closed reader: the exit status becomes 1.
pub(crate) static STDOUT_ERROR: AtomicBool = AtomicBool::new(false);

pub(crate) fn stdout_gone() -> bool {
    STDOUT_GONE.load(Ordering::Relaxed)
}

/// stdout went away. The program doesn't stop there: the rest of `run` (later `--cmd` results,
/// `--save`, `--export`) still happens, so `cadcraft-cli run … --save out.dxf | head` keeps the
/// drawing. A reader that stopped early (a closed pipe) is not an error, as in ripgrep; any other
/// write error is reported and makes the exit status 1.
pub(crate) fn stdout_failed(e: std::io::Error) {
    if STDOUT_GONE.swap(true, Ordering::Relaxed) || e.kind() == std::io::ErrorKind::BrokenPipe {
        return;
    }
    eprintln!("cadcraft-cli: can't write to stdout: {e}");
    STDOUT_ERROR.store(true, Ordering::Relaxed);
}

/// The model-space view of every open drawing, by drawing id (to tell whether commands changed it).
pub(crate) fn model_views(s: &Session) -> Vec<(u64, Option<View>)> {
    s.docs.iter().map(|st| (st.uid, st.views.iter().find(|(sp, _)| *sp == Space::Model).map(|(_, v)| *v))).collect()
}

/// The active drawing's current model-space view as a rectangle with the viewport's aspect ratio
/// (the CLI viewport is 3:2 like the PNG export, so the image shows exactly this view).
fn current_window(s: &Session) -> Option<Bounds2> {
    let st = s.state().ok()?;
    let v = st.views.iter().find(|(sp, _)| *sp == Space::Model).map(|(_, v)| *v)?;
    let (w, h) = s.viewport_px;
    let half = Vec2::new(v.height * w / h.max(1.0), v.height) * 0.5;
    Some(Bounds2::new(v.center - half, v.center + half))
}

/// The window an image export shows, or `None` to fit the extents. `before` is [`model_views`]
/// taken before the commands ran (`None` for `convert`, where nothing ran).
pub(crate) fn export_window(s: &Session, framing: Framing, before: Option<&[(u64, Option<View>)]>) -> Result<Option<Bounds2>, String> {
    match framing {
        Framing::Window(w) => Ok(Some(w)),
        Framing::Extents => Ok(None),
        Framing::Current => current_window(s).map(Some).ok_or_else(|| "--view current: the drawing has no model-space view".into()),
        Framing::Auto => {
            let Some(before) = before else { return Ok(None) };
            let Ok(st) = s.state() else { return Ok(None) };
            let now = model_views(s);
            let was = before.iter().find(|(uid, _)| *uid == st.uid);
            let is = now.iter().find(|(uid, _)| *uid == st.uid);
            // Only a view the commands changed counts; a drawing opened by the commands keeps
            // the extents framing.
            match (was, is) {
                (Some(a), Some(b)) if a.1 != b.1 => Ok(current_window(s)),
                _ => Ok(None),
            }
        }
    }
}

/// Write `bytes` to `out` through the engine's atomic writer (an exclusively created sibling file
/// renamed over `out`), so a failed or interrupted write leaves the previous file untouched.
pub(crate) fn write_atomic(out: &str, bytes: &[u8]) -> std::io::Result<()> {
    cadcraft_engine::atomic_write::write_atomic(std::path::Path::new(out), bytes)
}

pub(crate) fn export(s: &Session, out: &str) -> Result<(), String> {
    export_framed(s, out, None)
}

pub(crate) fn export_framed(s: &Session, out: &str, window: Option<Bounds2>) -> Result<(), String> {
    let d = s.doc().map_err(|e| e.to_string())?;
    let bytes = cadcraft_io::write_framed(d, out, window).map_err(|e| e.to_string())?;
    write_atomic(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
    eprintln!("wrote {out} ({} bytes)", bytes.len());
    if cadcraft_engine::cmd::file::drops_unknown(out) {
        let lost = cadcraft_engine::cmd::file::not_saved(d);
        if let Some(m) = cadcraft_engine::cmd::file::not_saved_message(&lost) {
            eprintln!("{m}");
        }
    }
    Ok(())
}

/// The warning for the model-space entities of `path` an image can't show, by type (those kept
/// as unknown objects, `Proxy` in `info`), or `None` when there are none.
pub(crate) fn undrawn_message(d: &cadcraft_engine::doc::Drawing, path: &str) -> Option<String> {
    let mut types: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for e in d.model.iter() {
        if let cadcraft_engine::doc::EntityKind::Unknown(u) = &e.kind {
            let n = types.entry(u.dxf_type.as_str()).or_insert(0);
            *n = n.saturating_add(1);
        }
    }
    let n = types.values().fold(0usize, |a, n| a.saturating_add(*n));
    if n == 0 {
        return None;
    }
    let list: Vec<String> = types.iter().map(|(t, n)| format!("{t}: {n}")).collect();
    let entities = if n == 1 { "entity" } else { "entities" };
    Some(format!("{path}: {n} {entities} not drawn ({})", list.join(", ")))
}

/// The warning for the records of `path` the reader could not place, or `None` when there are none.
pub(crate) fn skipped_message(d: &cadcraft_engine::doc::Drawing, path: &str) -> Option<String> {
    let n = d.skipped_records.values().fold(0usize, |a, n| a.saturating_add(*n));
    if n == 0 {
        return None;
    }
    let list: Vec<String> = d.skipped_records.iter().map(|(section, n)| format!("{section}: {n}")).collect();
    let records = if n == 1 { "record" } else { "records" };
    Some(format!("{path}: {n} {records} could not be read and are left out ({})", list.join(", ")))
}
