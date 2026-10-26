//! What the palettes say about the selection as a whole: how many objects of which types, and
//! the properties they share. It is worked out once per selection and drawing revision and kept
//! ([`Cache`]). The Properties and Quick Properties palettes are painted every frame, and scanning
//! the whole selection each time, several times over (once per shared property, with a string
//! per object), made the app crawl once every object of a large drawing was selected.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{Drawing, Handle};
use cadcraft_engine::DocState;

use crate::CadApp;

/// Shown for a property whose value differs between the selected objects, as AutoCAD's
/// Properties palette does.
pub(crate) const VARIES: &str = "*VARIES*";

/// The selection as the palettes present it.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Summary {
    /// The selected objects the drawing has (a stale handle is not counted).
    pub count: usize,
    /// Their types in first-seen order, with how many of each.
    pub types: Vec<(&'static str, usize)>,
    /// The colour they are all given, when it is the same one.
    pub color: Option<Color>,
    /// The colour they all draw in (their own, resolved through the layer), when it is the same
    /// one; white otherwise.
    pub resolved: Rgb,
    /// Layer, linetype, linetype scale, lineweight and transparency: the shared value, or
    /// [`VARIES`].
    pub layer: String,
    pub linetype: String,
    pub ltscale: String,
    pub lineweight: String,
    pub transparency: String,
    /// Tells this selection from another in widget ids, so a field being edited for one
    /// selection does not carry over to the next.
    pub fingerprint: u64,
}

/// A value every object seen so far agrees on, until one differs.
enum Shared<T> {
    Unset,
    Same(T),
    Varies,
}

impl<T: PartialEq> Shared<T> {
    fn see(&mut self, v: T) {
        match self {
            Shared::Unset => *self = Shared::Same(v),
            Shared::Same(s) if *s == v => {}
            Shared::Same(_) => *self = Shared::Varies,
            Shared::Varies => {}
        }
    }
    fn get(self) -> Option<T> {
        match self {
            Shared::Same(v) => Some(v),
            Shared::Unset | Shared::Varies => None,
        }
    }
    fn text(self, show: impl FnOnce(T) -> String) -> String {
        self.get().map_or_else(|| VARIES.to_string(), show)
    }
}

impl Summary {
    /// The summary of `selection` in `d`, which is drawing `drawing` (the uid, part of the
    /// fingerprint).
    pub(crate) fn of(d: &Drawing, selection: &[Handle], drawing: u64) -> Summary {
        let mut types: Vec<(&'static str, usize)> = Vec::new();
        let (mut color, mut resolved, mut layer, mut linetype) = (Shared::Unset, Shared::Unset, Shared::Unset, Shared::Unset);
        let (mut ltscale, mut lineweight, mut transparency) = (Shared::Unset, Shared::Unset, Shared::Unset);
        // A layer's colour is looked up once per layer, not once per object.
        let mut layer_colors: HashMap<&str, Color> = HashMap::new();
        let mut count = 0;
        for e in selection.iter().filter_map(|h| d.entity(*h)) {
            count += 1;
            let name = e.kind.type_name();
            match types.iter_mut().find(|(t, _)| *t == name) {
                Some((_, n)) => *n += 1,
                None => types.push((name, 1)),
            }
            let c = &e.common;
            let layer_color = *layer_colors.entry(c.layer.as_str()).or_insert_with(|| d.layer(&c.layer).map_or(Color::WHITE, |l| l.color));
            color.see(c.color);
            resolved.see(c.color.resolve(layer_color, Color::WHITE));
            layer.see(c.layer.as_str());
            linetype.see(c.linetype.as_str());
            ltscale.see(c.ltscale.to_bits());
            lineweight.see(c.lineweight);
            transparency.see(c.transparency);
        }
        Summary {
            count,
            types,
            color: color.get(),
            resolved: resolved.get().unwrap_or(Rgb(255, 255, 255)),
            layer: layer.text(str::to_string),
            linetype: linetype.text(str::to_string),
            ltscale: ltscale.text(|bits| format!("{:.4}", f64::from_bits(bits))),
            lineweight: lineweight.text(|lw| lw.name()),
            transparency: transparency.text(|tr| tr.name()),
            fingerprint: fingerprint(drawing, selection),
        }
    }
}

fn fingerprint(drawing: u64, selection: &[Handle]) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    drawing.hash(&mut h);
    selection.hash(&mut h);
    h.finish()
}

/// The summary of the current selection, kept while the drawing, its revision and the selection
/// stay the same.
#[derive(Default)]
pub(crate) struct Cache {
    /// The (drawing uid, revision, selection) the summary was made for.
    key: Option<(u64, u64, Vec<Handle>)>,
    summary: Arc<Summary>,
}

impl Cache {
    /// The summary of `st`'s selection: the kept one while nothing changed, else a fresh one.
    pub(crate) fn get(&mut self, st: &DocState) -> Arc<Summary> {
        let kept = self.key.as_ref().is_some_and(|(uid, revision, sel)| (*uid, *revision) == (st.uid, st.revision) && *sel == st.selection);
        if !kept {
            self.key = Some((st.uid, st.revision, st.selection.clone()));
            self.summary = Arc::new(Summary::of(&st.doc, &st.selection, st.uid));
        }
        self.summary.clone()
    }
}

/// The current selection's summary (see [`Cache`]); an empty one with no drawing open.
pub(crate) fn of_selection(app: &mut CadApp) -> Arc<Summary> {
    match app.session.state() {
        Ok(st) => app.summary.get(st),
        Err(_) => Arc::default(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::Services;
    use cadcraft_engine::Session;
    use serde_json::json;

    fn app_with_lines() -> (CadApp, Vec<String>) {
        let mut app = CadApp::new(Session::default(), Services::default());
        app.run("layer.new", json!({ "name": "Blue", "color": 5 })).unwrap();
        let a = app.run("line", json!({ "points": [[0, 0], [10, 0]] })).unwrap()["handles"][0].as_str().unwrap().to_string();
        let b = app.run("line", json!({ "points": [[0, 1], [10, 1]] })).unwrap()["handles"][0].as_str().unwrap().to_string();
        let c = app.run("circle", json!({ "center": [5, 5], "radius": 2 })).unwrap()["handle"].as_str().unwrap().to_string();
        app.run("properties.set", json!({ "handles": [&b], "layer": "Blue", "color": 1 })).unwrap();
        (app, vec![a, b, c])
    }

    #[test]
    fn shared_and_differing_properties() {
        let (mut app, hs) = app_with_lines();
        app.run("select", json!({ "handles": [&hs[0], &hs[2]] })).unwrap();
        let s = of_selection(&mut app);
        assert_eq!((s.count, s.types.clone()), (2, vec![("Line", 1), ("Circle", 1)]));
        assert_eq!((s.color, s.layer.as_str(), s.linetype.as_str()), (Some(Color::ByLayer), "0", "ByLayer"));
        assert_eq!((s.ltscale.as_str(), s.lineweight.as_str(), s.transparency.as_str()), ("1.0000", "ByLayer", "ByLayer"));
        assert_eq!(s.resolved, Rgb(255, 255, 255));

        app.run("select", json!({ "handles": [&hs[0], &hs[1]] })).unwrap();
        let s = of_selection(&mut app);
        assert_eq!((s.count, s.types.clone()), (2, vec![("Line", 2)]));
        assert_eq!((s.color, s.layer.as_str()), (None, VARIES));
        assert_eq!(s.resolved, Rgb(255, 255, 255), "a red line and a white one draw in no shared colour");

        app.run("select", json!({ "handles": [&hs[1]] })).unwrap();
        let s = of_selection(&mut app);
        assert_eq!((s.count, s.color, s.layer.as_str()), (1, Some(Color::Index(1)), "Blue"));
        assert_eq!(s.resolved, Rgb(255, 0, 0));
    }

    #[test]
    fn summary_is_kept_until_the_selection_or_drawing_changes() {
        let (mut app, hs) = app_with_lines();
        app.run("select", json!({ "handles": [&hs[0], &hs[1]] })).unwrap();
        let first = of_selection(&mut app);
        let again = of_selection(&mut app);
        assert!(Arc::ptr_eq(&first, &again), "nothing changed: the same summary is handed out");

        // The same selection in another order is another selection (its fingerprint tells
        // widget ids apart), with the same properties.
        app.run("select", json!({ "handles": [&hs[1], &hs[0]] })).unwrap();
        let reordered = of_selection(&mut app);
        assert!(!Arc::ptr_eq(&first, &reordered));
        assert_ne!(first.fingerprint, reordered.fingerprint);
        assert_eq!((reordered.count, &reordered.layer), (2, &first.layer));

        // An edit to a selected object: a fresh summary with the new value.
        app.run("properties.set", json!({ "handles": [&hs[0]], "layer": "Blue" })).unwrap();
        let edited = of_selection(&mut app);
        assert!(!Arc::ptr_eq(&reordered, &edited));
        assert_eq!(edited.layer, "Blue");

        app.run("ai_deselect", json!({})).unwrap();
        let none = of_selection(&mut app);
        assert_eq!((none.count, none.types.len(), none.color), (0, 0, None));
    }

    #[test]
    fn stale_handles_are_not_counted() {
        let (mut app, hs) = app_with_lines();
        app.run("select", json!({ "handles": [&hs[0], &hs[2]] })).unwrap();
        let d = Arc::make_mut(&mut app.session.state_mut().unwrap().doc);
        d.remove_entity(Handle::parse_hex(&hs[2]).unwrap());
        let s = of_selection(&mut app);
        assert_eq!((s.count, s.types.clone()), (1, vec![("Line", 1)]));
    }
}
