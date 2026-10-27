//! A text's formula (`=...`) round-trips through DXF as CADCRAFTFORMULA extended data, a long
//! Hebrew one included (cut into 250-byte strings between letters).

use cadcraft_doc::{Common, Drawing, EntityKind, HAlign, MText, Space, Text, VAlign};
use cadcraft_geom::Vec3;

#[test]
fn text_and_mtext_formulas_roundtrip() {
    let mut d = Drawing::new_metric();
    let long = format!("=\"{}\"", "שלום ".repeat(120));
    let t = Text {
        backwards: false,
        upside_down: false,
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
        frame: false,
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
