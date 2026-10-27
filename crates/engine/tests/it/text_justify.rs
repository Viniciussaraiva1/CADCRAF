//! Text and multiline text take another justification in place (the letters don't move), and
//! keep their Backwards / Upside down / Text frame settings.

use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use serde_json::json;

fn bounds(s: &Session, h: &str) -> cadcraft_engine::geom::Bounds2 {
    let d = s.doc().unwrap();
    let e = d.model.iter().find(|e| e.handle.hex() == h).unwrap();
    match &e.kind {
        EntityKind::Text(t) => cadcraft_render::place_text_entity(d, t, &t.value).1,
        EntityKind::MText(t) => cadcraft_render::layout_mtext_entity(d, t).bounds,
        _ => panic!("not text"),
    }
}

fn near(a: cadcraft_engine::geom::Bounds2, b: cadcraft_engine::geom::Bounds2) -> bool {
    a.min.dist(b.min) < 1e-6 && a.max.dist(b.max) < 1e-6
}

#[test]
fn text_justification_changes_keep_the_letters_in_place() {
    let mut s = Session::new();
    let r = s.execute("text", &json!({ "at": [10, 5], "height": 2.5, "text": "Hello 20", "rotation": 30 })).unwrap();
    let h = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
    let b0 = bounds(&s, &h);
    for j in ["MC", "TR", "BL", "C", "R", "M", "L"] {
        s.execute("properties.set", &json!({ "handles": [h], "justify": j })).unwrap();
        assert!(near(bounds(&s, &h), b0), "{j} moved the text");
    }
    let d = s.doc().unwrap();
    let EntityKind::Text(t) = &d.model.iter().next().unwrap().kind else { panic!() };
    assert_eq!(t.halign, cadcraft_engine::doc::HAlign::Left);
    assert!(s.execute("properties.set", &json!({ "handles": [h], "justify": "XX" })).is_err());
}

#[test]
fn backwards_upside_down_and_frame_are_properties() {
    let mut s = Session::new();
    let r = s.execute("text", &json!({ "at": [0, 0], "height": 1, "text": "AB" })).unwrap();
    let h = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
    s.execute("properties.set", &json!({ "handles": [h], "backwards": true, "upsideDown": true })).unwrap();
    let EntityKind::Text(t) = &s.doc().unwrap().model.iter().next().unwrap().kind else { panic!() };
    assert!(t.backwards && t.upside_down);

    let r = s.execute("mtext", &json!({ "at": [0, 10], "height": 1, "width": 20, "text": "Line one\\Pline two" })).unwrap();
    let m = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
    let b0 = bounds(&s, &m);
    s.execute("properties.set", &json!({ "handles": [m], "justify": "MC", "frame": true })).unwrap();
    assert!(near(bounds(&s, &m), b0), "multiline text stays in place");
    let d = s.doc().unwrap();
    let mt = d.model.iter().find_map(|e| match &e.kind {
        EntityKind::MText(t) => Some(t.clone()),
        _ => None,
    });
    let mt = mt.unwrap();
    assert!(mt.frame && mt.attach == 5);
    let frame = cadcraft_render::mtext_frame(d, &mt).unwrap();
    assert_eq!(frame.len(), 5);
}

#[test]
fn text_formulas_show_their_result_and_edit_as_formulas() {
    let mut s = Session::new();
    let r = s.execute("text", &json!({ "at": [0, 0], "height": 1, "text": "=2400-2*18" })).unwrap();
    let h = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
    let get = |s: &Session| match &s.doc().unwrap().model.iter().next().unwrap().kind {
        EntityKind::Text(t) => (t.value.clone(), t.formula.clone()),
        _ => panic!(),
    };
    assert_eq!(get(&s), ("2364".to_string(), Some("=2400-2*18".to_string())));
    // Edited as the formula; plain text drops it.
    s.execute("textedit", &json!({ "handle": h, "text": "=ROUND(10/3,2)" })).unwrap();
    assert_eq!(get(&s).0, "3.33");
    s.execute("textedit", &json!({ "handle": h, "text": "plain" })).unwrap();
    assert_eq!(get(&s), ("plain".to_string(), None));
    // Dates follow the clock when fields are updated.
    s.execute("textedit", &json!({ "handle": h, "text": "=TEXT(TODAY(),\"yyyy\")" })).unwrap();
    let year = get(&s).0;
    assert!(year.len() == 4 && year.starts_with("20"), "{year}");
    assert_eq!(s.execute("updatefield", &json!({})).unwrap()["updated"], 1);
}

#[test]
fn formulas_work_when_editing_multiline_text_dimensions_and_leaders() {
    let mut s = Session::new();
    let r = s.execute("mtext", &json!({ "at": [0, 10], "height": 1, "width": 20, "text": "abc" })).unwrap();
    let m = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
    s.execute("textedit", &json!({ "handle": m, "text": "{\\fArial|b0;=1200+350*2}" })).unwrap();
    let mt = s.doc().unwrap().model.iter().find_map(|e| match &e.kind {
        EntityKind::MText(t) => Some(t.clone()),
        _ => None,
    });
    let mt = mt.unwrap();
    assert!(cadcraft_fonts::plain_mtext(&mt.contents).contains("1900"), "{}", mt.contents);
    // Two lines: not a formula.
    s.execute("textedit", &json!({ "handle": m, "text": "=1+1\\Psecond" })).unwrap();
    let mt = s.doc().unwrap().model.iter().find_map(|e| match &e.kind {
        EntityKind::MText(t) => Some(t.clone()),
        _ => None,
    });
    assert!(mt.unwrap().formula.is_none());
}

#[test]
fn dimension_and_leader_text_take_formulas() {
    let mut s = Session::new();
    let r = s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [1250, 0], "at": [600, 100] })).unwrap();
    let h = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
    s.execute("textedit", &json!({ "handle": h, "text": "=<>/2" })).unwrap();
    let d = s.doc().unwrap();
    let EntityKind::Dimension(dm) = &d.model.iter().find(|e| e.handle.hex() == h).unwrap().kind else { panic!() };
    assert_eq!(dm.marks.formula.as_deref(), Some("=<>/2"));
    assert_eq!(dm.text, "625");
    assert_eq!(cadcraft_render::dimension_in(d, dm).value, "625");
    let r = s.execute("mleader", &json!({ "points": [[0, 0], [5, 5]], "text": "A" })).unwrap();
    let m = r["handle"].as_str().unwrap().to_string();
    s.execute("textedit", &json!({ "handle": m, "text": "=2*21" })).unwrap();
    let d = s.doc().unwrap();
    let EntityKind::MLeader(ml) = &d.model.iter().find(|e| e.handle.hex() == m).unwrap().kind else { panic!() };
    assert_eq!(ml.text.as_ref().unwrap().contents, "42");
}

/// properties.set replacing a text drops the formula it had (or takes the new one), like
/// typing over it; UPDATEFIELD does not bring the old result back (#2034 review).
#[test]
fn replacing_text_in_properties_updates_its_formula() {
    let mut s = Session::new();
    let r = s.execute("text", &json!({ "at": [0, 0], "height": 1, "text": "=1+1" })).unwrap();
    let h = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
    let r = s.execute("mtext", &json!({ "at": [0, 10], "height": 1, "width": 20, "text": "=2+2" })).unwrap();
    let m = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
    s.execute("properties.set", &json!({ "handles": [h, m], "text": "replacement" })).unwrap();
    s.execute("updatefield", &json!({})).unwrap();
    for e in s.doc().unwrap().model.iter() {
        match &e.kind {
            EntityKind::Text(t) => assert_eq!((t.value.as_str(), t.formula.as_deref()), ("replacement", None)),
            EntityKind::MText(t) => assert_eq!((t.contents.as_str(), t.formula.as_deref()), ("replacement", None)),
            _ => {}
        }
    }
    // A formula given there is worked out.
    s.execute("properties.set", &json!({ "handles": [h], "text": "=3*3" })).unwrap();
    let t = s.doc().unwrap().model.iter().find_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.clone()) } else { None }).unwrap();
    assert_eq!((t.value.as_str(), t.formula.as_deref()), ("9", Some("=3*3")));
}

/// SHEET() in a layout is that layout's name, in model space "Model", whichever is active when
/// fields are updated (#2034 review).
#[test]
fn sheet_is_each_texts_own_layout() {
    let mut s = Session::new();
    s.execute("text", &json!({ "at": [0, 0], "height": 1, "text": "=SHEET()" })).unwrap();
    let layout = s.doc().unwrap().layouts.first().map(|l| l.name.clone()).unwrap();
    s.execute("layout.set", &json!({ "name": layout })).unwrap();
    s.execute("text", &json!({ "at": [0, 0], "height": 1, "text": "=SHEET()" })).unwrap();
    s.execute("updatefield", &json!({})).unwrap();
    let d = s.doc().unwrap();
    let value = |e: &std::sync::Arc<cadcraft_engine::doc::Entity>| if let EntityKind::Text(t) = &e.kind { t.value.clone() } else { String::new() };
    assert_eq!(d.model.iter().map(value).collect::<Vec<_>>(), ["Model"]);
    let l = d.layouts.iter().find(|l| l.name == layout).unwrap();
    assert_eq!(l.entities.iter().map(value).collect::<Vec<_>>(), std::slice::from_ref(&layout));
}

/// properties.set refuses a justification the selected object type does not have (multiline
/// text has no baseline justifications) instead of reporting a change it did not make.
#[test]
fn a_justification_must_suit_the_selected_objects() {
    let mut s = Session::new();
    let r = s.execute("mtext", &json!({ "at": [0, 10], "height": 1, "width": 20, "text": "abc" })).unwrap();
    let m = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
    assert!(s.execute("properties.set", &json!({ "handles": [m], "justify": "C" })).is_err());
    assert!(s.execute("properties.set", &json!({ "handles": [m], "justify": "TL" })).is_ok());
}

/// MLEADERSTYLE sets the style's arrowhead: a built-in one, a block, or back to closed filled.
#[test]
fn a_multileader_styles_arrowhead_is_set_by_the_command() {
    let mut s = Session::new();
    let arrow = |s: &Session| s.doc().unwrap().mleader_styles.iter().find(|m| m.name == "Standard").map(|m| m.arrow_block.clone());
    s.execute("mleaderstyle", &json!({ "name": "Standard", "arrowBlock": "_Open" })).unwrap();
    assert_eq!(arrow(&s).as_deref(), Some("_Open"));
    assert!(s.execute("mleaderstyle", &json!({ "name": "Standard", "arrowBlock": "NOSUCHBLOCK" })).is_err());
    s.execute("mleaderstyle", &json!({ "name": "Standard", "arrowBlock": "" })).unwrap();
    assert_eq!(arrow(&s).as_deref(), Some(""));
}

/// A missing big font is reported with the drawing's other missing fonts.
#[test]
fn a_missing_big_font_is_reported() {
    let mut s = Session::new();
    {
        let d = s.doc_mut().unwrap();
        let mut st = d.text_styles.first().cloned().unwrap();
        st.name = "ASIAN".into();
        st.font = "txt.shx".into();
        st.big_font = "cadcraft_no_such_bigfont.shx".into();
        d.text_styles.push(st);
    }
    let r = s.execute("fonts.missing", &json!({})).unwrap();
    let fonts: Vec<&str> = r.as_array().unwrap().iter().filter_map(|f| f["font"].as_str()).collect();
    assert!(fonts.contains(&"cadcraft_no_such_bigfont.shx"), "{fonts:?}");
    assert!(!fonts.contains(&"txt.shx"), "the stroke font stands in for txt.shx");
}

/// A new multileader's text that is a formula is worked out as when it is edited; a multiline
/// result keeps `\`, `{` and `}` as letters (#2034 review).
#[test]
fn new_multileader_formulas_and_literal_results() {
    let mut s = Session::new();
    s.execute("mleader", &json!({ "points": [[0, 0], [10, 10]], "text": "=2*3" })).unwrap();
    let ml = s.doc().unwrap().model.iter().find_map(|e| if let EntityKind::MLeader(m) = &e.kind { m.text.clone() } else { None }).unwrap();
    assert_eq!((cadcraft_fonts::plain_mtext(&ml.contents).as_str(), ml.formula.as_deref()), ("6", Some("=2*3")));
    // In multiline text a brace is written `\{`: the formula is =UPPER("{a}").
    let r = s.execute("mtext", &json!({ "at": [0, 20], "height": 1, "width": 20, "text": "=UPPER(\"\\{a\\}\")" })).unwrap();
    let m = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
    let mt = s.doc().unwrap().entity(cadcraft_engine::doc::Handle::parse_hex(&m).unwrap()).map(|e| e.kind.clone());
    let Some(EntityKind::MText(mt)) = mt else { panic!() };
    assert_eq!(mt.formula.as_deref(), Some("=UPPER(\"{a}\")"));
    assert_eq!(cadcraft_fonts::plain_mtext(&mt.contents), "{A}", "{}", mt.contents);
}

/// A dimension formula's `<>` is the measurement itself, not the first number in the shown text:
/// a DIMPOST prefix with a digit (`2X<>`) does not become the value (#2034 review).
#[test]
fn a_dimension_formula_uses_the_measurement_not_the_shown_text() {
    let mut s = Session::new();
    s.execute("setvar", &json!({ "name": "DIMPOST", "value": "2X<>" })).unwrap();
    let r = s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [1250, 0], "at": [600, 100] })).unwrap();
    let h = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
    s.execute("textedit", &json!({ "handle": h, "text": "=<>/2" })).unwrap();
    let d = s.doc().unwrap();
    let EntityKind::Dimension(dm) = &d.model.iter().find(|e| e.handle.hex() == h).unwrap().kind else { panic!() };
    assert_eq!(cadcraft_render::dimension_in(d, dm).value, "625");
}
