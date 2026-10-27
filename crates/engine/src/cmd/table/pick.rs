//! Picking the cells a table-cell command works on: the table and cell a command names, what a
//! scope covers, and the prompts that pick a cell and its scope at the command line.

use cadcraft_doc::{Handle, Table};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::anchors;
use super::cells::cell_under;
use crate::cmd::bad;
use crate::{Accept, Input, Prompt, Result, Session};

/// The table and cell a command names: `at` (a point in a cell), else `handle` + `row` + `col`.
pub(super) fn target(s: &Session, p: &Value, id: &str) -> Result<Value> {
    let mut q = p.clone();
    if let Some(at) = p.get("at").and_then(|v| serde_json::from_value::<[f64; 2]>(v.clone()).ok()) {
        let (h, r, c) = cell_under(s, Vec2::new(at[0], at[1])).ok_or_else(|| bad(id, "no table cell at that point"))?;
        q["handle"] = json!(h.hex());
        q["row"] = json!(r);
        q["col"] = json!(c);
    }
    Ok(q)
}

/// The anchor of the merge covering cell (`r`, `c`), or the cell itself.
pub(super) fn anchor_of(t: &Table, r: usize, c: usize) -> (usize, usize) {
    anchors(t).into_iter().find(|&(ar, ac, rs, cs)| r >= ar && r < ar + rs && c >= ac && c < ac + cs).map(|(ar, ac, ..)| (ar, ac)).unwrap_or((r, c))
}

/// Which cells a scope covers, from cell (`r`, `c`): (first row, rows, first column, columns).
/// Column, row and table scopes cover data rows only (below the title and header rows).
/// With `labels`, column and table scopes take in the title and header rows too.
pub(super) fn scope_cells(
    t: &Table,
    scope: &str,
    r: usize,
    c: usize,
    rows: usize,
    cols: usize,
    labels: bool,
) -> std::result::Result<(usize, usize, usize, usize), String> {
    let (nr, nc) = (t.cells.len(), t.col_widths.len());
    let data = if labels { 0 } else { (usize::from(t.title) + usize::from(t.header)).min(nr) };
    Ok(match scope.trim().to_ascii_lowercase().as_str() {
        "" | "cell" => (r, 1, c, 1),
        "column" | "col" => (data, nr - data, c, 1),
        "row" => (r, 1, 0, nc),
        "table" => (data, nr - data, 0, nc),
        "range" => (r, rows.max(1).min(nr - r), c, cols.max(1).min(nc - c)),
        other => return Err(format!("unknown scope {other} (cell, column, row, table, range)")),
    })
}

/// Picking cells at the command line: a cell, what it stands for (the cell, its column, its row,
/// the table, or a range to a second cell).
#[derive(Default)]
pub(super) struct CellPick {
    pub(super) cell: Option<(Handle, usize, usize)>,
    pub(super) scope: Option<String>,
    pub(super) corner: Option<(usize, usize)>,
}

/// Where a [`CellPick`] is after an input.
pub(super) enum Picked {
    Asking,
    Done,
    Cancel,
}

const SCOPES: &[&str] = &["Cell", "cOlumn", "Row", "Table", "rAnge"];

impl CellPick {
    /// The prompt while picking, or `None` once picked.
    pub(super) fn prompt(&self) -> Option<Prompt> {
        match (&self.cell, self.scope.as_deref()) {
            (None, _) => Some(Prompt::new("Pick a table cell", Accept::POINT)),
            (Some(_), None) => Some(Prompt::new("Apply to", Accept::TEXT).kw(SCOPES).default("Cell")),
            (Some(_), Some("range")) if self.corner.is_none() => Some(Prompt::new("Pick the opposite cell of the range", Accept::POINT)),
            _ => None,
        }
    }
    pub(super) fn input(&mut self, s: &mut Session, i: Input) -> Picked {
        match (&self.cell, self.scope.as_deref(), i) {
            (_, _, Input::Cancel) | (None, _, Input::Enter) => return Picked::Cancel,
            (None, _, Input::Point(p)) => match cell_under(s, p) {
                Some(c) => self.cell = Some(c),
                None => s.echo("No table cell there."),
            },
            (Some(_), None, Input::Enter) => self.scope = Some("cell".into()),
            (Some(_), None, Input::Keyword(k) | Input::Text(k)) => {
                let k = k.trim().to_ascii_lowercase();
                let scope = match k.as_str() {
                    "o" => Some("column"),
                    "a" => Some("range"),
                    _ => ["cell", "column", "row", "table", "range"].into_iter().find(|x| !k.is_empty() && x.starts_with(&k)),
                };
                match scope {
                    Some(sc) => self.scope = Some(sc.into()),
                    None => s.echo("Choose Cell, cOlumn, Row, Table or rAnge."),
                }
            }
            (Some((h, ..)), Some("range"), Input::Point(p)) => match cell_under(s, p) {
                Some((h2, r2, c2)) if h2 == *h => self.corner = Some((r2, c2)),
                _ => s.echo("Pick a cell of the same table."),
            },
            _ => {}
        }
        if self.prompt().is_none() { Picked::Done } else { Picked::Asking }
    }
    /// The picked cells as command parameters: handle, row, col, scope (and rows, cols).
    pub(super) fn params(&self) -> serde_json::Map<String, Value> {
        let mut q = serde_json::Map::new();
        let Some((h, r, c)) = self.cell else { return q };
        let (r0, c0) = self.corner.map_or((r, c), |(r2, c2)| (r.min(r2), c.min(c2)));
        q.insert("handle".into(), json!(h.hex()));
        q.insert("row".into(), json!(r0));
        q.insert("col".into(), json!(c0));
        q.insert("scope".into(), json!(self.scope.clone().unwrap_or_else(|| "cell".into())));
        if let Some((r2, c2)) = self.corner {
            q.insert("rows".into(), json!(r.abs_diff(r2) + 1));
            q.insert("cols".into(), json!(c.abs_diff(c2) + 1));
        }
        q
    }
}
