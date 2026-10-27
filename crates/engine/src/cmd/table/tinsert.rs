//! TINSERT: a block in a table cell, with attribute values and a dynamic block's visibility
//! state. At the command line: pick the cell, then name the block.

use std::collections::BTreeMap;

use cadcraft_doc::dynblock::{ParameterKind, dynamic_block};
use cadcraft_doc::{CellBlock, Drawing, Handle, MAX_CELL_BLOCK_VALUES};
use serde_json::{Value, json};

use super::cells::cell_under;
use super::pick::{anchor_of, target};
use super::{cell_at, edit};
use crate::cmd::{CommandSpec, bad, str_param};
use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

const TINSERT_PARAMS: &str = "{handle?, row+col | at: [x,y], block: name (\"\" removes the block), scale?: number | \"fit\" (default fit), \
     rotation?: degrees, attribs?: {TAG: value}, props?: {Visibility1: state}} → {handle}";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("tinsert", "Insert Block in Table Cell", run_tinsert)
            .params(TINSERT_PARAMS)
            .interactive(|_| Ok(Box::new(TinsertM::default()))),
    ]
}

/// A JSON object of strings (numbers and booleans as typed), at most [`MAX_CELL_BLOCK_VALUES`].
fn string_map(p: &Value, key: &str) -> std::result::Result<BTreeMap<String, String>, String> {
    let Some(v) = p.get(key).filter(|v| !v.is_null()) else { return Ok(BTreeMap::new()) };
    let o = v.as_object().ok_or_else(|| format!("`{key}` must be an object of names and values"))?;
    if o.len() > MAX_CELL_BLOCK_VALUES {
        return Err(format!("`{key}` has more than {MAX_CELL_BLOCK_VALUES} entries"));
    }
    Ok(o.iter()
        .map(|(k, v)| (k.trim().to_string(), v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())))
        .filter(|(k, _)| !k.is_empty())
        .collect())
}

/// Check custom properties `props` against block `name`: each must name a visibility parameter
/// of the dynamic block and one of its states (other properties change the block's geometry,
/// which cells can't show yet).
fn check_props(d: &Drawing, name: &str, props: &BTreeMap<String, String>) -> std::result::Result<(), String> {
    if props.is_empty() {
        return Ok(());
    }
    let m = dynamic_block(d, name).ok_or_else(|| format!("Block \"{name}\" is not a dynamic block."))?;
    let vis: Vec<(&str, Vec<&str>)> = m
        .parameters
        .iter()
        .filter_map(|p| match &p.kind {
            ParameterKind::Visibility { label, states, .. } => Some((label.as_str(), states.iter().map(|s| s.name.as_str()).collect())),
            _ => None,
        })
        .collect();
    for (k, v) in props {
        let found =
            vis.iter().find(|(l, _)| l.eq_ignore_ascii_case(k)).or_else(|| k.eq_ignore_ascii_case("Visibility").then(|| vis.first()).flatten());
        let Some((label, states)) = found else {
            return Err(format!("A table cell can set only a visibility state of \"{name}\"; it has no visibility property \"{k}\"."));
        };
        if !states.iter().any(|s| s.eq_ignore_ascii_case(v.trim())) {
            return Err(format!("\"{v}\" is not a state of {label}: {}.", states.join(", ")));
        }
    }
    Ok(())
}

fn run_tinsert(s: &mut Session, p: &Value) -> Result<Value> {
    let q = target(s, p, "tinsert")?;
    let name = str_param(&q, "block").map(str::trim).unwrap_or("").to_string();
    let block = if name.is_empty() {
        None
    } else {
        let d = s.doc()?;
        let real = d.block(&name).map(|b| b.name.clone()).ok_or_else(|| bad("tinsert", format!("Block \"{name}\" not found.")))?;
        let scale = match q.get("scale") {
            None | Some(Value::Null) => None,
            Some(Value::String(f)) if f.trim().eq_ignore_ascii_case("fit") => None,
            Some(v) => {
                Some(v.as_f64().filter(|x| x.is_finite() && *x > 0.0 && *x <= 1e9).ok_or_else(|| bad("tinsert", format!("invalid scale {v}")))?)
            }
        };
        let rotation = match q.get("rotation") {
            None | Some(Value::Null) => 0.0,
            Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad("tinsert", format!("invalid rotation {v}")))?.to_radians(),
        };
        let attribs = string_map(&q, "attribs").map_err(|m| bad("tinsert", m))?;
        let props = string_map(&q, "props").map_err(|m| bad("tinsert", m))?;
        check_props(d, &real, &props).map_err(|m| bad("tinsert", m))?;
        Some(CellBlock { name: real, scale, rotation, attribs, props }.sanitized())
    };
    edit(s, &q, "tinsert", |t| {
        let (r, c) = cell_at(&q, t)?;
        let (r, c) = anchor_of(t, r, c);
        if let Some(cell) = t.cells.get_mut(r).and_then(|row| row.get_mut(c)) {
            if block.is_some() {
                // A cell holds a block or text, not both (as in AutoCAD).
                cell.text.clear();
                cell.formula = None;
            }
            cell.block = block;
        }
        Ok(())
    })
}

/// TINSERT at the command line: pick a cell, then name the block (fitted into the cell).
#[derive(Default)]
struct TinsertM {
    cell: Option<(Handle, usize, usize)>,
}

impl Interactive for TinsertM {
    fn name(&self) -> &'static str {
        "TINSERT"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.cell {
            None => Prompt::new("Pick a table cell", Accept::POINT),
            Some(_) => Prompt::new("Enter block name", Accept::TEXT),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match (&self.cell, i) {
            (_, Input::Cancel) | (None, Input::Enter) | (Some(_), Input::Enter) => Ok(Step::Cancel),
            (None, Input::Point(p)) => {
                match cell_under(s, p) {
                    Some(c) => self.cell = Some(c),
                    None => s.echo("No table cell there."),
                }
                Ok(Step::Continue)
            }
            (Some((h, r, c)), Input::Text(name) | Input::Keyword(name)) => {
                let q = json!({ "handle": h.hex(), "row": r, "col": c, "block": name });
                match run_tinsert(s, &q) {
                    Ok(_) => Ok(Step::Done),
                    Err(EngineError::BadParams { msg, .. }) => {
                        s.echo(msg);
                        Ok(Step::Continue)
                    }
                    Err(e) => Err(e),
                }
            }
            _ => Ok(Step::Continue),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_maps_are_bounded() {
        let mut big = serde_json::Map::new();
        for i in 0..=MAX_CELL_BLOCK_VALUES {
            big.insert(format!("T{i}"), json!(i));
        }
        assert!(string_map(&json!({ "attribs": big }), "attribs").is_err());
        let m = string_map(&json!({ "attribs": { "NUMBER": 5, " ": "x", "VAL": "a" } }), "attribs").unwrap_or_default();
        assert_eq!(m.get("NUMBER").map(String::as_str), Some("5"));
        assert_eq!(m.len(), 2);
        assert!(string_map(&json!({ "attribs": [1] }), "attribs").is_err());
    }
}
