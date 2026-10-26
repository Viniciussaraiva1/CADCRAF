//! The preservation contract for this DXF test slice: ordered model-space entities and their
//! referenced tables. Handles and every Common/LINE/CIRCLE/TEXT field are checked. Only TEXT
//! angles allow roundoff from radians ↔ degrees; unsupported entity kinds fail rather than skip.
//! Header defaults, generated objects and other drawing collections are outside this slice.

use std::fmt::Debug;

use cadcraft_doc::{Circle, Common, Drawing, Entity, EntityKind, Line, Text};

fn equal<T: PartialEq + Debug>(expected: &T, actual: &T, context: &str, field: &str) {
    assert_eq!(actual, expected, "{context}.{field}");
}

fn angle(expected: f64, actual: f64, context: &str, field: &str) {
    // Two floating-point unit conversions, not a geometric approximation or angle wrapping.
    let tolerance = 8.0 * f64::EPSILON * expected.abs().max(1.0);
    assert!(
        expected.is_finite() && actual.is_finite() && (actual - expected).abs() <= tolerance,
        "{context}.{field}: expected {expected:?}, actual {actual:?}, tolerance {tolerance:e}"
    );
}

pub(super) fn model(expected: &Drawing, actual: &Drawing, context: &str) {
    equal(&expected.model.len(), &actual.model.len(), context, "model.len");
    for (index, (a, b)) in expected.model.iter().zip(actual.model.iter()).enumerate() {
        let path = format!("{context} · model.entity[{index}]");
        entity(a, b, &path);
        equal(&expected.layer(&a.common.layer), &actual.layer(&a.common.layer), &path, "referenced_layer");
        let linetype = |d: &Drawing| d.linetypes.iter().find(|l| l.name == a.common.linetype).cloned();
        equal(&linetype(expected), &linetype(actual), &path, "referenced_linetype");
        if let EntityKind::Text(text) = &a.kind {
            equal(&expected.text_style(&text.style), &actual.text_style(&text.style), &path, "referenced_text_style");
        }
    }
}

fn entity(expected: &Entity, actual: &Entity, context: &str) {
    let Entity { handle, common: properties, kind } = expected;
    equal(handle, &actual.handle, context, "handle");
    common(properties, &actual.common, &format!("{context}.common"));
    match (kind, &actual.kind) {
        (EntityKind::Line(Line { a, b }), EntityKind::Line(other)) => {
            equal(a, &other.a, context, "line.a");
            equal(b, &other.b, context, "line.b");
        }
        (EntityKind::Circle(Circle { center, radius }), EntityKind::Circle(other)) => {
            equal(center, &other.center, context, "circle.center");
            equal(radius, &other.radius, context, "circle.radius");
        }
        (EntityKind::Text(a), EntityKind::Text(b)) => text(a, b, &format!("{context}.text")),
        _ => panic!("{context}.kind: unsupported or changed type: expected {kind:?}, actual {:?}", actual.kind),
    }
}

fn common(expected: &Common, actual: &Common, context: &str) {
    // No `..`: adding a field requires deciding its preservation contract here.
    let Common { layer, color, linetype, lineweight, ltscale, transparency, visible, thickness, extrusion, plot_style, anno_scales, xdata } =
        expected;
    equal(layer, &actual.layer, context, "layer");
    equal(color, &actual.color, context, "color");
    equal(linetype, &actual.linetype, context, "linetype");
    equal(lineweight, &actual.lineweight, context, "lineweight");
    equal(ltscale, &actual.ltscale, context, "ltscale");
    equal(transparency, &actual.transparency, context, "transparency");
    equal(visible, &actual.visible, context, "visible");
    equal(thickness, &actual.thickness, context, "thickness");
    equal(extrusion, &actual.extrusion, context, "extrusion");
    equal(plot_style, &actual.plot_style, context, "plot_style");
    equal(anno_scales, &actual.anno_scales, context, "anno_scales");
    equal(xdata, &actual.xdata, context, "xdata");
}

fn text(expected: &Text, actual: &Text, context: &str) {
    let Text { insert, align_pt, height, value, rotation, width_factor, oblique, style, halign, valign } = expected;
    equal(insert, &actual.insert, context, "insert");
    equal(align_pt, &actual.align_pt, context, "align_pt");
    equal(height, &actual.height, context, "height");
    equal(value, &actual.value, context, "value");
    angle(*rotation, actual.rotation, context, "rotation");
    equal(width_factor, &actual.width_factor, context, "width_factor");
    angle(*oblique, actual.oblique, context, "oblique");
    equal(style, &actual.style, context, "style");
    equal(halign, &actual.halign, context, "halign");
    equal(valign, &actual.valign, context, "valign");
}

#[test]
fn comparison_rejects_lost_fields_and_changed_handles() {
    use cadcraft_doc::Handle;
    use cadcraft_geom::Vec3;

    let original = Entity::new(Handle(0x123), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(2.0, 3.0, 4.0) }));
    let mutations: [fn(&mut Entity); 4] = [
        |e| e.handle = Handle(0x456),
        |e| e.common.visible = false,
        |e| e.common.xdata.push(cadcraft_doc::RawTag { code: 1000, value: "unexpected".into() }),
        |e| e.kind = EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 2.0 }),
    ];
    for mutate in mutations {
        let mut changed = original.clone();
        mutate(&mut changed);
        assert!(std::panic::catch_unwind(|| entity(&original, &changed, "comparison self-test")).is_err());
    }
}

#[test]
fn angle_tolerance_does_not_hide_real_changes_or_nonfinite_values() {
    angle(0.7, 0.7_f64.to_degrees().to_radians(), "self-test", "angle");
    for actual in [0.700001, f64::NAN, f64::INFINITY] {
        assert!(std::panic::catch_unwind(|| angle(0.7, actual, "self-test", "angle")).is_err());
    }
}
