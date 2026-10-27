//! TABLESIZE: column widths, row heights and how rows grow for their text. At the command line:
//! pick the cells, then their columns' width and their rows' height.

use cadcraft_doc::EntityKind;
use serde_json::{Value, json};

use super::pick::{CellPick, Picked, scope_cells, target};
use super::{cell_at, edit, row_grow_param, usize_param};
use crate::cmd::{CommandSpec, bad, str_param};
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

const SIZE_PARAMS: &str = "{handle?, row+col | at: [x,y], scope?: cell|column|row|table|range (default cell), rows?, cols? (range), \
     width? (the range's columns), height? (its rows), widths?: [w, …] (columns from `col`, default 0), heights?: [h, …] (rows from `row`, default 0), \
     rowGrow?: free|fixed|step (how rows grow for their text: freely, never, or to multiples of the step)} → {handle}";

/// Largest column width or row height (drawing units).
const MAX_SIZE: f64 = 1e6;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("tablesize", "Table Column Width and Row Height", run_size)
            .alias(&["tsi"])
            .params(SIZE_PARAMS)
            .interactive(|_| Ok(Box::new(SizeM::default()))),
    ]
}

/// A positive size no larger than [`MAX_SIZE`].
fn size_value(v: &Value, what: &str) -> std::result::Result<f64, String> {
    v.as_f64().filter(|x| x.is_finite() && *x > 0.0 && *x <= MAX_SIZE).ok_or_else(|| format!("invalid {what} {v} (more than 0, at most {MAX_SIZE})"))
}

/// A list of sizes, at most `max` of them.
fn size_list(p: &Value, key: &str, max: usize) -> std::result::Result<Option<Vec<f64>>, String> {
    let Some(v) = p.get(key).filter(|v| !v.is_null()) else { return Ok(None) };
    let a = v.as_array().ok_or_else(|| format!("`{key}` must be a list of sizes"))?;
    if a.len() > max {
        return Err(format!("`{key}` has more entries than the table has"));
    }
    a.iter().map(|x| size_value(x, key)).collect::<std::result::Result<Vec<_>, _>>().map(Some)
}

fn run_size(s: &mut Session, p: &Value) -> Result<Value> {
    let q = target(s, p, "tablesize")?;
    let one = |k: &str| q.get(k).filter(|v| !v.is_null()).map(|v| size_value(v, k)).transpose();
    let width = one("width").map_err(|m| bad("tablesize", m))?;
    let height = one("height").map_err(|m| bad("tablesize", m))?;
    let scope = str_param(&q, "scope").unwrap_or("cell").to_string();
    let grow = row_grow_param(&q).map_err(|m| bad("tablesize", m))?;
    edit(s, &q, "tablesize", |t| {
        let (nr, nc) = (t.row_heights.len(), t.col_widths.len());
        let widths = size_list(&q, "widths", nc)?;
        let heights = size_list(&q, "heights", nr)?;
        if width.is_none() && height.is_none() && widths.is_none() && heights.is_none() && grow.is_none() {
            return Err("nothing to set: give width, height, widths, heights or rowGrow".into());
        }
        if let Some(g) = grow {
            t.row_grow = g;
        }
        if let Some(ws) = widths {
            let c0 = usize_param(&q, "col").unwrap_or(0).min(nc);
            for (w, slot) in ws.iter().zip(t.col_widths.iter_mut().skip(c0)) {
                *slot = *w;
            }
        }
        if let Some(hs) = heights {
            let r0 = usize_param(&q, "row").unwrap_or(0).min(nr);
            for (i, h) in hs.iter().enumerate() {
                t.set_row_height(r0 + i, *h);
            }
        }
        if width.is_some() || height.is_some() {
            let (r, c) = cell_at(&q, t)?;
            let (r0, rows, c0, cols) =
                scope_cells(t, &scope, r, c, usize_param(&q, "rows").unwrap_or(1), usize_param(&q, "cols").unwrap_or(1), true)?;
            if let Some(w) = width {
                for slot in t.col_widths.iter_mut().skip(c0).take(cols) {
                    *slot = w;
                }
            }
            if let Some(h) = height {
                for r in r0..(r0 + rows).min(nr) {
                    t.set_row_height(r, h);
                }
            }
        }
        Ok(())
    })
}

/// `v` with at most 4 decimals and no trailing zeros ("8", "6.5").
fn short_number(v: f64) -> String {
    let t = format!("{v:.4}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// TABLESIZE at the command line: pick the cells, then their columns' width and their rows'
/// height (Enter keeps each); `Grow` at the height sets how the table's rows grow for their
/// text.
#[derive(Default)]
struct SizeM {
    pick: CellPick,
    /// 0 width, 1 height, 2 row growth.
    step: u8,
    params: serde_json::Map<String, Value>,
}

impl SizeM {
    /// The first picked column's width or row's height, as the prompt's default.
    fn current(&self, s: &Session) -> Option<f64> {
        let (h, r, c) = self.pick.cell?;
        let d = s.doc().ok()?;
        let EntityKind::Table(t) = &d.entity(h)?.kind else { return None };
        if self.step == 0 { t.col_widths.get(c).copied() } else { Some(t.row_base(r)) }
    }
}

impl Interactive for SizeM {
    fn name(&self) -> &'static str {
        "TABLESIZE"
    }
    fn prompt(&self, s: &Session) -> Prompt {
        if let Some(p) = self.pick.prompt() {
            return p;
        }
        if self.step == 2 {
            return Prompt::new("Rows grow for their text: Free, Fixed or a step (Enter: keep)", Accept::TEXT).kw(GROWS).default(self.grow_now(s));
        }
        let p = if self.step == 0 {
            Prompt::new("Column width (Enter: keep)", Accept::NUMBER)
        } else {
            Prompt::new("Row height (Enter: keep)", Accept::NUMBER).kw(&["Grow"])
        };
        match self.current(s) {
            Some(v) => p.default(short_number(v)),
            None => p,
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.pick.prompt().is_some() {
            return Ok(match self.pick.input(s, i) {
                Picked::Cancel => Step::Cancel,
                _ => Step::Continue,
            });
        }
        if self.step == 2 {
            match i {
                Input::Cancel => return Ok(Step::Cancel),
                Input::Text(t) | Input::Keyword(t) => match row_grow_param(&json!({ "rowGrow": t.trim() })) {
                    Ok(Some(g)) => {
                        self.params.insert("rowGrow".into(), json!(grow_word(g)));
                    }
                    _ => {
                        s.echo("Type Free, Fixed or a step more than 0.");
                        return Ok(Step::Continue);
                    }
                },
                _ => {}
            }
            return Ok(self.finish(s));
        }
        let key = if self.step == 0 { "width" } else { "height" };
        match i {
            Input::Cancel => return Ok(Step::Cancel),
            Input::Text(t) | Input::Keyword(t) if self.step == 1 && t.trim().eq_ignore_ascii_case("grow") => {
                self.step = 2;
                return Ok(Step::Continue);
            }
            Input::Enter => {}
            Input::Text(t) | Input::Keyword(t) => match t.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0 && *v <= MAX_SIZE) {
                Some(v) => {
                    self.params.insert(key.into(), json!(v));
                }
                None => {
                    s.echo("Type a size more than 0.");
                    return Ok(Step::Continue);
                }
            },
            _ => return Ok(Step::Continue),
        }
        if self.step == 0 {
            self.step = 1;
            return Ok(Step::Continue);
        }
        Ok(self.finish(s))
    }
}

const GROWS: &[&str] = &["Free", "Fixed"];

/// A row growth as TABLESIZE's `rowGrow` takes it.
fn grow_word(g: cadcraft_doc::RowGrow) -> String {
    match g {
        cadcraft_doc::RowGrow::Free => "free".into(),
        cadcraft_doc::RowGrow::Fixed => "fixed".into(),
        cadcraft_doc::RowGrow::Step(s) => short_number(s),
    }
}

impl SizeM {
    /// The picked table's row growth, as the prompt's default ("Free", "Fixed", "8").
    fn grow_now(&self, s: &Session) -> String {
        let g = self.pick.cell.and_then(|(h, ..)| match s.doc().ok()?.entity(h).map(|e| &e.kind) {
            Some(EntityKind::Table(t)) => Some(t.row_grow),
            _ => None,
        });
        match g.unwrap_or_default() {
            cadcraft_doc::RowGrow::Free => "Free".into(),
            cadcraft_doc::RowGrow::Fixed => "Fixed".into(),
            cadcraft_doc::RowGrow::Step(v) => short_number(v),
        }
    }

    /// Apply what was typed.
    fn finish(&self, s: &mut Session) -> Step {
        if !self.params.is_empty() {
            let mut q = self.pick.params();
            q.extend(self.params.clone());
            if let Err(e) = run_size(s, &Value::Object(q)) {
                s.echo(e.to_string());
            }
        }
        Step::Done
    }
}
