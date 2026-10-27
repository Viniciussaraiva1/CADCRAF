//! A text's formula (`=...`) round-trips through DXF as CADCRAFTFORMULA extended data, a long
//! Hebrew one included (cut into 250-byte strings between letters).

use cadcraft_doc::{Common, Drawing, EntityKind, HAlign, MText, Space, Text, VAlign};
use cadcraft_geom::Vec3;

#[test]
fn text_and_mtext_formulas_roundtrip() {
    let mut d = Drawing::new_metric();
    let long = format!("=\"{}\"", "שלום ".repeat(120));
    let t = Text {
        insert: Vec3::new(1.0, 2.0, 0.0),
        align_pt: None,
        height: 2.5,
        value: "2".into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::Left,
        valign: VAlign::Baseline,
        formula: Some("=1+1".into()),
    };
    d.add(&Space::Model, Common::default(), EntityKind::Text(t)).unwrap();
    let m = MText {
        formula: Some(long.clone()),
        insert: Vec3::new(0.0, 10.0, 0.0),
        height: 2.5,
        width: 40.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "x".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
    };
    d.add(&Space::Model, Common::default(), EntityKind::MText(m)).unwrap();
    let bytes = cadcraft_io::write(&d, "a.dxf").unwrap();
    // The xdata application is registered (AutoCAD refuses xdata of an unknown application).
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("APPID") && text.matches("CADCRAFTFORMULA").count() >= 3, "registered once, used twice");
    let back = cadcraft_io::read(&bytes, "a.dxf").unwrap();
    let kinds: Vec<EntityKind> = back.model.iter().map(|e| e.kind.clone()).collect();
    assert!(kinds.iter().any(|k| matches!(k, EntityKind::Text(t) if t.formula.as_deref() == Some("=1+1"))));
    assert!(kinds.iter().any(|k| matches!(k, EntityKind::MText(m) if m.formula.as_deref() == Some(long.as_str()))));
}

fn text(value: &str, formula: Option<&str>) -> Text {
    Text {
        insert: Vec3::new(0.0, 0.0, 0.0),
        align_pt: None,
        height: 2.5,
        value: value.into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::Left,
        valign: VAlign::Baseline,
        formula: formula.map(str::to_string),
    }
}

fn mtext(contents: &str, formula: Option<&str>) -> MText {
    MText {
        formula: formula.map(str::to_string),
        insert: Vec3::new(0.0, 10.0, 0.0),
        height: 2.5,
        width: 40.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: contents.into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
    }
}

/// Attribute definitions, attributes and multileaders keep their formulas too (#2318 review).
#[test]
fn attribute_and_multileader_formulas_roundtrip() {
    let mut d = Drawing::new_metric();
    let def = cadcraft_doc::Attrib {
        tag: "T".into(),
        text: text("2", Some("=1+1")),
        invisible: false,
        constant: false,
        prompt: String::new(),
        props: Default::default(),
        multiline: None,
        modes: Default::default(),
    };
    d.add(&Space::Model, Common::default(), EntityKind::AttDef(def.clone())).unwrap();
    d.blocks.insert("B".into(), cadcraft_doc::Block::new("B").into());
    let ins = cadcraft_doc::Insert {
        block: "B".into(),
        insert: Vec3::new(5.0, 0.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: vec![cadcraft_doc::Attrib { text: text("6", Some("=2*3")), ..def }],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    };
    d.add(&Space::Model, Common::default(), EntityKind::Insert(ins)).unwrap();
    let ml = cadcraft_doc::MLeader {
        leaders: vec![vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 0.0)]],
        landing: Vec3::new(10.0, 10.0, 0.0),
        dogleg: 2.0,
        text: Some(mtext("9", Some("=3*3"))),
        style: "Standard".into(),
        arrow_size: 1.0,
        spline: false,
        block: None,
    };
    d.add(&Space::Model, Common::default(), EntityKind::MLeader(ml)).unwrap();
    let bytes = cadcraft_io::write(&d, "a.dxf").unwrap();
    let back = cadcraft_io::read(&bytes, "a.dxf").unwrap();
    let mut found = Vec::new();
    for e in back.model.iter() {
        match &e.kind {
            EntityKind::AttDef(a) => found.push(a.text.formula.clone()),
            EntityKind::Insert(i) => found.extend(i.attribs.iter().map(|a| a.text.formula.clone())),
            EntityKind::MLeader(m) => found.push(m.text.as_ref().and_then(|t| t.formula.clone())),
            _ => {}
        }
    }
    found.sort();
    assert_eq!(found, [Some("=1+1".to_string()), Some("=2*3".to_string()), Some("=3*3".to_string())]);
}

/// A formula replaced by plain text after reopening stays gone: its old extended data is not
/// kept as another program's data and written back (#2318 review).
#[test]
fn a_cleared_formula_is_not_restored_from_old_xdata() {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Text(text("2", Some("=1+1")))).unwrap();
    d.add(&Space::Model, Common::default(), EntityKind::MText(mtext("4", Some("=2+2")))).unwrap();
    let bytes = cadcraft_io::write(&d, "a.dxf").unwrap();
    let mut back = cadcraft_io::read(&bytes, "a.dxf").unwrap();
    let hs: Vec<_> = back.model.iter().map(|e| e.handle).collect();
    for h in hs {
        back.modify_entity(h, |e| match &mut e.kind {
            EntityKind::Text(t) => {
                t.value = "plain".into();
                t.formula = None;
            }
            EntityKind::MText(m) => {
                m.contents = "plain".into();
                m.formula = None;
            }
            _ => {}
        })
        .unwrap();
    }
    let again = cadcraft_io::read(&cadcraft_io::write(&back, "a.dxf").unwrap(), "a.dxf").unwrap();
    for e in again.model.iter() {
        match &e.kind {
            EntityKind::Text(t) => assert_eq!((t.value.as_str(), t.formula.as_deref()), ("plain", None)),
            EntityKind::MText(m) => assert_eq!((m.contents.as_str(), m.formula.as_deref()), ("plain", None)),
            _ => {}
        }
    }
}
