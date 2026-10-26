//! Named, finite input matrices for LINE, CIRCLE and TEXT. Single-property sweeps detect omitted
//! fields; small interaction products exercise shared properties and text alignment. These are
//! valid model values, not fuzz input. Every expected entity is constructed before serialization.

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{
    AnnoScale, Circle, Common, Drawing, EntityKind, HAlign, Layer, Line, Linetype, Lineweight, RawTag, Space, Text, TextStyle, Transparency, VAlign,
};
use cadcraft_geom::Vec3;

use super::check;

fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    d.layers.push(Layer { name: "Survey_Ä".into(), color: Color::Index(3), ..Layer::default() });
    d.linetypes.push(Linetype::simple("Dash_test", "Original synthetic dash pattern", &[2.0, -0.75, 0.0, -0.25]));
    d.text_styles.push(TextStyle { name: "Notes".into(), ..TextStyle::default() });
    d
}

fn text() -> Text {
    Text {
        insert: Vec3::new(2.5, -3.75, 4.125),
        align_pt: None,
        height: 2.25,
        value: "CAD Ø".into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::Left,
        valign: VAlign::Baseline,
    }
}

fn kinds() -> [EntityKind; 3] {
    [
        EntityKind::Line(Line { a: Vec3::new(-1.25, 2.5, 3.75), b: Vec3::new(8.125, -13.5, 21.25) }),
        EntityKind::Circle(Circle { center: Vec3::new(-7.25, 11.5, 4.75), radius: 3.125 }),
        EntityKind::Text(text()),
    ]
}

fn check_kind(case: &str, common: Common, kind: EntityKind) {
    let mut d = drawing();
    d.add(&Space::Model, common, kind).unwrap();
    check(case, &d);
}

fn check_common(case: &str, common: Common) {
    let mut d = drawing();
    for kind in kinds() {
        d.add(&Space::Model, common.clone(), kind).unwrap();
    }
    check(case, &d);
}

#[test]
fn empty_model_and_default_properties() {
    check("empty-model", &drawing());
    check_common("common/default", Common::default());
}

#[test]
fn line_endpoints_and_circle_centers_radii() {
    for (name, a, b) in [
        ("zero-start", Vec3::ZERO, Vec3::new(1.0, 2.0, 3.0)),
        ("zero-end", Vec3::new(-3.0, -2.0, -1.0), Vec3::ZERO),
        ("small", Vec3::new(1e-9, -2e-9, 3e-9), Vec3::new(-4e-9, 5e-9, -6e-9)),
        ("survey", Vec3::new(1e7 + 0.125, -2e7 - 0.25, 123.75), Vec3::new(1e7 + 0.5, -2e7 + 0.75, -42.5)),
    ] {
        for (direction, start, end) in [("forward", a, b), ("reverse", b, a)] {
            check_kind(&format!("line/{name}/{direction}"), Common::default(), EntityKind::Line(Line { a: start, b: end }));
        }
        for radius in [1e-9, 0.125, 3.25, 1e8] {
            check_kind(&format!("circle/{name}/radius={radius}"), Common::default(), EntityKind::Circle(Circle { center: a, radius }));
        }
    }
}

#[test]
fn common_properties_individually() {
    for layer in ["0", "Survey_Ä"] {
        check_common(&format!("common/layer={layer}"), Common { layer: layer.into(), ..Common::default() });
    }
    for color in [Color::ByLayer, Color::ByBlock, Color::Index(1), Color::Index(255), Color::True(Rgb(0, 0, 0)), Color::True(Rgb(17, 93, 241))] {
        check_common(&format!("common/color={color:?}"), Common { color, ..Common::default() });
    }
    for linetype in ["ByLayer", "ByBlock", "Continuous", "Dash_test"] {
        check_common(&format!("common/linetype={linetype}"), Common { linetype: linetype.into(), ..Common::default() });
    }
    for lineweight in [Lineweight::ByLayer, Lineweight::ByBlock, Lineweight::Default].into_iter().chain(Lineweight::STANDARD.map(Lineweight::Mm100)) {
        check_common(&format!("common/lineweight={lineweight:?}"), Common { lineweight, ..Common::default() });
    }
    for ltscale in [0.125, 1.0, 3.25, 1e6] {
        check_common(&format!("common/ltscale={ltscale}"), Common { ltscale, ..Common::default() });
    }
    for transparency in [Transparency::ByLayer, Transparency::ByBlock].into_iter().chain((0..=90).map(Transparency::Percent)) {
        check_common(&format!("common/transparency={transparency:?}"), Common { transparency, ..Common::default() });
    }
    for visible in [false, true] {
        check_common(&format!("common/visible={visible}"), Common { visible, ..Common::default() });
    }
    for thickness in [-2.5, 0.0, 3.75] {
        check_common(&format!("common/thickness={thickness}"), Common { thickness, ..Common::default() });
    }
    // ByBlock has a separate pending preservation test below (known writer loss).
    for plot_style in ["ByLayer", "Review"] {
        check_common(&format!("common/plot_style={plot_style}"), Common { plot_style: plot_style.into(), ..Common::default() });
    }
    let xdata = vec![
        RawTag { code: 1001, value: "ROUNDTRIP_TEST".into() },
        RawTag { code: 1000, value: "foreign text Ø 😀".into() },
        RawTag { code: 1070, value: "-17".into() },
        RawTag { code: 1040, value: "3.25".into() },
        RawTag { code: 1004, value: "00FF80".into() },
    ];
    check_common("common/foreign-xdata", Common { xdata, ..Common::default() });
}

#[test]
fn common_property_interactions() {
    // Full local product: inheritance modes × visibility × layer. Deliberately combine other
    // non-default properties too, rather than assuming their individual sweeps cover interaction.
    for layer in ["0", "Survey_Ä"] {
        for color in [Color::ByLayer, Color::ByBlock, Color::True(Rgb(21, 89, 233))] {
            for visible in [false, true] {
                let common = Common {
                    layer: layer.into(),
                    color,
                    visible,
                    linetype: "Dash_test".into(),
                    lineweight: Lineweight::Mm100(53),
                    ltscale: 2.75,
                    transparency: Transparency::Percent(37),
                    thickness: -1.25,
                    plot_style: "Review".into(),
                    ..Common::default()
                };
                check_common(&format!("common/product/{layer}/{color:?}/visible={visible}"), common);
            }
        }
    }
}

#[test]
fn text_values_and_individual_fields() {
    for (name, value) in
        [("empty", ""), ("whitespace", "  a\tb  "), ("unicode", "Café 图号 😀 e\u{301}"), ("literal-escapes", r"\U+0041 \M+183A4 \P %%c")]
    {
        check_kind(&format!("text/value/{name}"), Common::default(), EntityKind::Text(Text { value: value.into(), ..text() }));
    }
    for height in [1e-9, 0.125, 2.25, 1e5] {
        check_kind(&format!("text/height={height}"), Common::default(), EntityKind::Text(Text { height, ..text() }));
    }
    for rotation in [0.0, -0.7, std::f64::consts::FRAC_PI_2, 5.9] {
        check_kind(&format!("text/rotation={rotation}"), Common::default(), EntityKind::Text(Text { rotation, ..text() }));
    }
    for width_factor in [0.125, 1.0, 2.75] {
        check_kind(&format!("text/width_factor={width_factor}"), Common::default(), EntityKind::Text(Text { width_factor, ..text() }));
    }
    for oblique in [-0.3, 0.0, 0.4] {
        check_kind(&format!("text/oblique={oblique}"), Common::default(), EntityKind::Text(Text { oblique, ..text() }));
    }
    for style in ["Standard", "Notes"] {
        check_kind(&format!("text/style={style}"), Common::default(), EntityKind::Text(Text { style: style.into(), ..text() }));
    }
    for scales in [vec![], vec![AnnoScale::unit()], vec![AnnoScale::unit(), AnnoScale::new("1:50", 1.0, 50.0)]] {
        check_kind(&format!("text/anno_scales={scales:?}"), Common { anno_scales: scales, ..Common::default() }, EntityKind::Text(text()));
    }
}

#[test]
fn text_alignment_and_optional_points() {
    for halign in [HAlign::Left, HAlign::Center, HAlign::Right, HAlign::Aligned, HAlign::Middle, HAlign::Fit] {
        for valign in [VAlign::Baseline, VAlign::Bottom, VAlign::Middle, VAlign::Top] {
            // ALIGNED, MIDDLE and FIT are baseline-only DXF horizontal alignment modes.
            if matches!(halign, HAlign::Aligned | HAlign::Middle | HAlign::Fit) && valign != VAlign::Baseline {
                continue;
            }
            for align_pt in [None, Some(Vec3::ZERO), Some(Vec3::new(17.25, -23.5, 6.75))] {
                // Left-baseline TEXT has no meaningful second alignment point in DXF. Its
                // explicit-but-ignored group 11 is tested separately, not normalized here.
                if halign == HAlign::Left && valign == VAlign::Baseline && align_pt.is_some() {
                    continue;
                }
                let mut t = Text { halign, valign, align_pt, rotation: 0.7, oblique: -0.2, width_factor: 1.25, style: "Notes".into(), ..text() };
                // Construct a consistent model using the font layout API, not the DXF writer.
                // Justified TEXT stores both its anchor and baseline start; stale baseline
                // points are intentionally recomputed on export. Compare both fields exactly.
                if let Some(anchor) = align_pt
                    && !matches!(halign, HAlign::Aligned | HAlign::Fit)
                {
                    let (h, v) = cadcraft_render::text_align(halign, valign);
                    let params = cadcraft_fonts::TextParams {
                        insert: t.insert.xy(),
                        align_pt: Some(anchor.xy()),
                        height: t.height,
                        rotation: t.rotation,
                        width_factor: t.width_factor,
                        oblique: t.oblique,
                        h,
                        v,
                        backwards: false,
                        upside_down: false,
                    };
                    t.insert = cadcraft_fonts::baseline_start(&cadcraft_fonts::TextFont::Stroke, &t.value, &params).to3(anchor.z);
                }
                check_kind(&format!("text/alignment/{halign:?}/{valign:?}/point={align_pt:?}"), Common::default(), EntityKind::Text(t));
            }
        }
    }
}

#[test]
#[ignore = "Known writer loss: ByBlock plot style becomes ByLayer on the first R2000 ASCII save"]
fn plot_style_byblock_is_preserved() {
    check_common("common/plot_style=ByBlock", Common { plot_style: "ByBlock".into(), ..Common::default() });
}

#[test]
#[ignore = "Known writer loss: groups 210/220/230 are omitted; a LINE's normal becomes (0,0,1)"]
fn nondefault_extrusion_is_preserved() {
    for extrusion in [Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, 1.0, 0.0)] {
        check_common(&format!("common/extrusion={extrusion:?}"), Common { extrusion, ..Common::default() });
    }
}
