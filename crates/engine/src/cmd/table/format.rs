//! TABLEFORMAT: a table cell's own alignment, margins, text height and text rotation, over its
//! cell style's. At the command line: pick the cells, then each value.

use cadcraft_doc::{CellLook, CellTurn};
use serde_json::{Value, json};

use super::cells::align_value;
use super::pick::{CellPick, Picked, anchor_of, scope_cells, target};
use super::{cell_at, edit, usize_param};
use crate::cmd::{CommandSpec, bad, str_param};
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

const FORMAT_PARAMS: &str = "{handle?, row+col | at: [x,y], scope?: cell|column|row|table|range (default cell; column, row and table: \
     their data rows, below the title and header), rows?, cols? (range), align?: TL|TC|TR|ML|MC|MR|BL|BC|BR, textHeight?, margin?, marginH?, marginV?, \
     rotation?: 0|90|180|270, clear?: bool (back to the cell style)} → {handle, cells}";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("tableformat", "Format Table Cells", run_format)
            .alias(&["tfo"])
            .params(FORMAT_PARAMS)
            .interactive(|_| Ok(Box::new(FormatM::default()))),
    ]
}

/// The look a format command sets: its alignment, margins and rotation (each when given).
fn look_param(p: &Value) -> std::result::Result<CellLook, String> {
    let margin = |k: &str| -> std::result::Result<Option<f64>, String> {
        match p.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_f64().filter(|x| x.is_finite() && (0.0..=1e9).contains(x)).map(Some).ok_or_else(|| format!("invalid {k}: {v}")),
        }
    };
    let both = margin("margin")?;
    let align = match p.get("align") {
        None | Some(Value::Null) => None,
        Some(a) => Some(align_value(a).ok_or_else(|| format!("invalid align {a} (TL, TC, TR, ML, MC, MR, BL, BC, BR)"))?),
    };
    let rotation = match p.get("rotation") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_u64()
                .and_then(|r| u16::try_from(r).ok())
                .and_then(CellTurn::from_degrees)
                .ok_or_else(|| format!("invalid rotation {v} (0, 90, 180, 270)"))?,
        ),
    };
    Ok(CellLook { align, margin_h: margin("marginH")?.or(both), margin_v: margin("marginV")?.or(both), rotation })
}

/// The scale of the table a command names: its text height over its table style's (1 when
/// unknown), as the renderer applies it to cell styles and cells' own text heights.
fn table_scale(s: &Session, q: &Value) -> f64 {
    let Ok((_, t)) = super::find(s, q, "tableformat") else { return 1.0 };
    let Ok(d) = s.doc() else { return 1.0 };
    let k = t.text_height / cadcraft_render::table_style_of(d, &t).text_height;
    if k.is_finite() && k > 0.0 { k } else { 1.0 }
}

fn run_format(s: &mut Session, p: &Value) -> Result<Value> {
    let q = target(s, p, "tableformat")?;
    let clear = q.get("clear").and_then(Value::as_bool).unwrap_or(false);
    let set = look_param(&q).map_err(|m| bad("tableformat", m))?;
    let height = match q.get("textHeight") {
        None | Some(Value::Null) => None,
        Some(v) => {
            Some(v.as_f64().filter(|h| h.is_finite() && *h > 0.0 && *h <= 1e6).ok_or_else(|| bad("tableformat", format!("invalid textHeight {v}")))?)
        }
    };
    if !clear && set.is_empty() && height.is_none() {
        return Err(bad("tableformat", "nothing to set: give align, textHeight, margin, marginH, marginV or rotation (or clear)"));
    }
    // A cell's own text height is kept before the table's scale (its text height over its
    // style's), so the height asked for is the one drawn.
    let scale = table_scale(s, &q);
    let scope = str_param(&q, "scope").unwrap_or("cell").to_string();
    let mut n = 0usize;
    let r = edit(s, &q, "tableformat", |t| {
        let (r, c) = cell_at(&q, t)?;
        let (r0, rows, c0, cols) = scope_cells(t, &scope, r, c, usize_param(&q, "rows").unwrap_or(1), usize_param(&q, "cols").unwrap_or(1), false)?;
        for r in r0..r0 + rows {
            for c in c0..c0 + cols {
                let (ar, ac) = anchor_of(t, r, c);
                let Some(cell) = t.cells.get_mut(ar).and_then(|row| row.get_mut(ac)) else { continue };
                let mut l = if clear { CellLook::default() } else { cell.look.unwrap_or_default() };
                l.align = set.align.or(l.align);
                l.margin_h = set.margin_h.or(l.margin_h);
                l.margin_v = set.margin_v.or(l.margin_v);
                l.rotation = set.rotation.or(l.rotation);
                cell.look = (!l.is_empty()).then_some(l);
                if clear {
                    cell.text_height = None;
                }
                if let Some(h) = height {
                    cell.text_height = Some(h / scale);
                }
                n += 1;
            }
        }
        Ok(())
    })?;
    Ok(json!({ "handle": r["handle"], "cells": n }))
}

/// TABLEFORMAT at the command line: pick the cells, then the alignment, margin and rotation
/// (Enter keeps each as it is).
#[derive(Default)]
struct FormatM {
    pick: CellPick,
    /// 0 alignment, 1 text height, 2 margin, 3 rotation.
    step: u8,
    params: serde_json::Map<String, Value>,
}

const ALIGNS: &[&str] = &["TL", "TC", "TR", "ML", "MC", "MR", "BL", "BC", "BR"];
const ROTATIONS: &[&str] = &["0", "90", "180", "270"];

impl FormatM {
    fn apply(&mut self, s: &mut Session) -> Result<Step> {
        if self.params.is_empty() {
            return Ok(Step::Done);
        }
        let mut q = self.pick.params();
        q.extend(self.params.clone());
        if let Err(e) = run_format(s, &Value::Object(q)) {
            s.echo(e.to_string());
        }
        Ok(Step::Done)
    }
}

impl Interactive for FormatM {
    fn name(&self) -> &'static str {
        "TABLEFORMAT"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if let Some(p) = self.pick.prompt() {
            return p;
        }
        match self.step {
            0 => Prompt::new("Alignment (Enter: keep)", Accept::TEXT).kw(ALIGNS),
            1 => Prompt::new("Text height (Enter: keep)", Accept::NUMBER),
            2 => Prompt::new("Margin from the cell's sides (Enter: keep)", Accept::NUMBER),
            _ => Prompt::new("Text rotation (Enter: keep)", Accept::TEXT).kw(ROTATIONS),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.pick.prompt().is_some() {
            return Ok(match self.pick.input(s, i) {
                Picked::Cancel => Step::Cancel,
                _ => Step::Continue,
            });
        }
        match (self.step, i) {
            (_, Input::Cancel) => return Ok(Step::Cancel),
            (3, Input::Enter) => return self.apply(s),
            (_, Input::Enter) => self.step += 1,
            (0, Input::Keyword(k) | Input::Text(k)) => match align_value(&json!(k)) {
                Some(a) => {
                    self.params.insert("align".into(), json!(a));
                    self.step = 1;
                }
                None => s.echo("Choose TL, TC, TR, ML, MC, MR, BL, BC or BR."),
            },
            (1, Input::Keyword(t) | Input::Text(t)) => match t.trim().parse::<f64>().ok().filter(|h| h.is_finite() && *h > 0.0 && *h <= 1e6) {
                Some(h) => {
                    self.params.insert("textHeight".into(), json!(h));
                    self.step = 2;
                }
                None => s.echo("The text height is more than 0."),
            },
            (2, Input::Keyword(t) | Input::Text(t)) => match t.trim().parse::<f64>().ok().filter(|m| m.is_finite() && (0.0..=1e9).contains(m)) {
                Some(m) => {
                    self.params.insert("marginH".into(), json!(m));
                    self.step = 3;
                }
                None => s.echo("The margin is a distance of 0 or more."),
            },
            (_, Input::Keyword(k) | Input::Text(k)) => match k.trim().parse::<u16>().ok().and_then(CellTurn::from_degrees) {
                Some(r) => {
                    self.params.insert("rotation".into(), json!(r.degrees()));
                    return self.apply(s);
                }
                None => s.echo("Choose 0, 90, 180 or 270."),
            },
            _ => {}
        }
        Ok(Step::Continue)
    }
}
