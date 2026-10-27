//! TABLEBORDERS: borders of a table cell's own, drawn over its cell style's. At the command line:
//! pick the cells, which of their edges, then the lineweight.

use cadcraft_doc::CellBorder;
use serde_json::{Value, json};

use super::cells::apply_border;
use super::pick::{CellPick, Picked, anchor_of, scope_cells, target};
use super::{cell_at, edit, usize_param};
use crate::cmd::{CommandSpec, bad, str_param};
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

const BORDERS_PARAMS: &str = "{handle?, row+col | at: [x,y], scope?: cell|column|row|table|range (default range: rows, cols), rows? (default 1), cols? (default 1), \
     edges?: all|outside|inside|top|bottom|left|right|insideH|insideV or a list (default all), \
     visible?: bool, color?, lineweight? (mm or ByBlock/ByLayer), clear?: bool (back to the cell style)} → {handle}";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("tableborders", "Set Table Cell Borders", run_borders)
            .alias(&["tbo"])
            .params(BORDERS_PARAMS)
            .interactive(|_| Ok(Box::new(BordersM::default()))),
    ]
}

/// Which edges of a range `edges` names: (top, bottom, left, right, inside horizontal, inside
/// vertical).
fn edge_set(v: Option<&Value>) -> std::result::Result<[bool; 6], String> {
    let names: Vec<String> = match v {
        None | Some(Value::Null) => vec!["all".into()],
        Some(Value::String(s)) => s.split(',').map(|x| x.trim().to_ascii_lowercase()).collect(),
        Some(Value::Array(a)) => a.iter().map(|x| x.as_str().unwrap_or("").trim().to_ascii_lowercase()).collect(),
        Some(v) => return Err(format!("invalid edges {v}")),
    };
    let mut out = [false; 6];
    for n in names {
        let set: &[usize] = match n.as_str() {
            "all" => &[0, 1, 2, 3, 4, 5],
            "outside" => &[0, 1, 2, 3],
            "inside" => &[4, 5],
            "top" => &[0],
            "bottom" => &[1],
            "left" => &[2],
            "right" => &[3],
            "insideh" => &[4],
            "insidev" => &[5],
            _ => return Err(format!("unknown edge {n} (all, outside, inside, top, bottom, left, right, insideH, insideV)")),
        };
        for i in set {
            if let Some(x) = out.get_mut(*i) {
                *x = true;
            }
        }
    }
    Ok(out)
}

fn run_borders(s: &mut Session, p: &Value) -> Result<Value> {
    let q = target(s, p, "tableborders")?;
    let p = &q;
    let which = edge_set(p.get("edges")).map_err(|m| bad("tableborders", m))?;
    let scope = str_param(p, "scope").unwrap_or("range").to_string();
    let clear = p.get("clear").and_then(Value::as_bool).unwrap_or(false);
    let mut border = CellBorder::default();
    if !clear {
        apply_border(&mut border, p).map_err(|m| bad("tableborders", m))?;
    }
    edit(s, p, "tableborders", |t| {
        let (r, c) = cell_at(p, t)?;
        // Borders of a column or the table take in the title and header rows too.
        let (r0, rows, c0, cols) = scope_cells(t, &scope, r, c, usize_param(p, "rows").unwrap_or(1), usize_param(p, "cols").unwrap_or(1), true)?;
        let [top, bottom, left, right, inside_h, inside_v] = which;
        let value = (!clear).then_some(border);
        for r in r0..r0 + rows {
            for c in c0..c0 + cols {
                let (ar, ac) = anchor_of(t, r, c);
                let Some(cell) = t.cells.get_mut(ar).and_then(|row| row.get_mut(ac)) else { continue };
                let mut e = cell.edges.unwrap_or_default();
                if (r == r0 && top) || (r > r0 && inside_h) {
                    e.top = value;
                }
                if (r + 1 == r0 + rows && bottom) || (r + 1 < r0 + rows && inside_h) {
                    e.bottom = value;
                }
                if (c == c0 && left) || (c > c0 && inside_v) {
                    e.left = value;
                }
                if (c + 1 == c0 + cols && right) || (c + 1 < c0 + cols && inside_v) {
                    e.right = value;
                }
                cell.edges = (!e.is_empty()).then_some(e);
            }
        }
        Ok(())
    })
}

/// TABLEBORDERS at the command line: pick the cells, which of their edges, then the lineweight
/// (millimetres, ByBlock, or Clear to go back to the cell style).
#[derive(Default)]
struct BordersM {
    pick: CellPick,
    edges: Option<String>,
}

const EDGES: &[&str] = &["All", "Outside", "Inside", "Top", "Bottom", "Left", "Right", "insideH", "insideV"];

impl Interactive for BordersM {
    fn name(&self) -> &'static str {
        "TABLEBORDERS"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if let Some(p) = self.pick.prompt() {
            return p;
        }
        match self.edges {
            None => Prompt::new("Edges", Accept::TEXT).kw(EDGES).default("All"),
            Some(_) => Prompt::new("Lineweight in mm, ByBlock or Clear", Accept::TEXT).kw(&["Byblock", "Clear"]).default("0.5"),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.pick.prompt().is_some() {
            return Ok(match self.pick.input(s, i) {
                Picked::Cancel => Step::Cancel,
                _ => Step::Continue,
            });
        }
        if matches!(i, Input::Cancel) {
            return Ok(Step::Cancel);
        }
        let Some(edges) = self.edges.clone() else {
            let k = match i {
                Input::Enter => "all".to_string(),
                Input::Keyword(k) | Input::Text(k) => k.trim().to_ascii_lowercase(),
                _ => return Ok(Step::Continue),
            };
            let name = match k.as_str() {
                "a" => Some("all"),
                "o" => Some("outside"),
                "i" => Some("inside"),
                "t" => Some("top"),
                "b" => Some("bottom"),
                "l" => Some("left"),
                "r" => Some("right"),
                "h" => Some("insideH"),
                "v" => Some("insideV"),
                _ => EDGES.iter().copied().find(|e| e.eq_ignore_ascii_case(&k)),
            };
            match name {
                Some(n) => self.edges = Some(n.to_string()),
                None => s.echo("Choose All, Outside, Inside, Top, Bottom, Left, Right, insideH or insideV."),
            }
            return Ok(Step::Continue);
        };
        let v = match i {
            Input::Enter => "0.5".to_string(),
            Input::Keyword(k) | Input::Text(k) => k.trim().to_ascii_lowercase(),
            _ => return Ok(Step::Continue),
        };
        let mut q = self.pick.params();
        q.insert("edges".into(), json!(edges));
        match v.as_str() {
            "c" | "clear" => {
                q.insert("clear".into(), json!(true));
            }
            "b" | "byblock" => {
                q.insert("lineweight".into(), json!("ByBlock"));
            }
            mm => match mm.trim_end_matches("mm").trim().parse::<f64>() {
                Ok(w) => {
                    q.insert("lineweight".into(), json!(w));
                }
                Err(_) => {
                    s.echo("Type a lineweight in millimetres (0.18, 0.5, 1), ByBlock or Clear.");
                    return Ok(Step::Continue);
                }
            },
        }
        if let Err(e) = run_borders(s, &Value::Object(q)) {
            s.echo(e.to_string());
            return Ok(Step::Continue);
        }
        Ok(Step::Done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_names() {
        assert_eq!(edge_set(None), Ok([true; 6]));
        assert_eq!(edge_set(Some(&json!("outside"))), Ok([true, true, true, true, false, false]));
        assert_eq!(edge_set(Some(&json!(["top", "insideV"]))), Ok([true, false, false, false, false, true]));
        assert!(edge_set(Some(&json!("diagonal"))).is_err());
        assert!(edge_set(Some(&json!(3))).is_err());
    }
}
