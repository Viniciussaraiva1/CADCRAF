//! Property, symbol-table and identity inputs. Values are authored before serialization;
//! each family can fail independently without suppressing unrelated preservation checks.

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{Common, DashElement, Entity, EntityKind, Handle, Layer, Line, Lineweight, RawTag, Text, TextStyle};
use cadcraft_geom::{Vec2, Vec3};

use super::{Case, common_case, drawing, entity_case, text};

pub(crate) fn mixed_case_references(kind: &str) -> Vec<Case> {
    let mut common = Common::default();
    let mut t = text();
    match kind {
        "layer" => common.layer = "sURVEY_Ä".into(),
        "linetype" => common.linetype = "dASH_TEST".into(),
        "style" => t.style = "nOTES".into(),
        _ => panic!("unknown reference family"),
    }
    vec![entity_case(&format!("reference/{kind}/mixed-case"), common, EntityKind::Text(t))]
}

fn layer_case(name: &str, layer: Layer) -> Case {
    let mut case = common_case(name, Common { layer: "Survey_Ä".into(), ..Common::default() });
    *case.drawing.layer_mut("Survey_Ä").unwrap() = Layer { name: "Survey_Ä".into(), ..layer };
    case
}

pub(crate) fn layer_flags() -> Vec<Case> {
    (0..32)
        .map(|bits| {
            layer_case(
                &format!("layer/flags={bits}"),
                Layer {
                    on: bits & 1 == 0,
                    frozen: bits & 2 != 0,
                    locked: bits & 4 != 0,
                    plot: bits & 8 == 0,
                    vp_freeze_new: bits & 16 != 0,
                    ..Layer::default()
                },
            )
        })
        .collect()
}

pub(crate) fn layer_appearance() -> Vec<Case> {
    [Color::Index(1), Color::Index(255), Color::True(Rgb(17, 93, 241))]
        .into_iter()
        .flat_map(|color| {
            [0, 37, 90].map(|transparency| {
                layer_case(
                    &format!("layer/{color:?}/alpha={transparency}"),
                    Layer {
                        color,
                        transparency,
                        linetype: "Dash_test".into(),
                        lineweight: Lineweight::Mm100(53),
                        description: "Synthetic survey Ø 图".into(),
                        ..Layer::default()
                    },
                )
            })
        })
        .collect()
}

pub(crate) fn layer_plot_styles() -> Vec<Case> {
    ["Normal", "Review"]
        .map(|plot_style| layer_case(&format!("layer/plot-style={plot_style}"), Layer { plot_style: plot_style.into(), ..Layer::default() }))
        .into()
}

fn style_case(name: &str, style: TextStyle) -> Case {
    let mut case = entity_case(name, Common::default(), EntityKind::Text(Text { style: "Notes".into(), ..text() }));
    *case.drawing.text_styles.iter_mut().find(|s| s.name == "Notes").unwrap() = TextStyle { name: "Notes".into(), ..style };
    case
}

pub(crate) fn text_style_metrics() -> Vec<Case> {
    [0.0, 0.125, 3.5]
        .into_iter()
        .flat_map(|height| {
            [0.5, 1.0, 2.75].map(|width_factor| {
                style_case(
                    &format!("style/height={height}/width={width_factor}"),
                    TextStyle { height, width_factor, oblique: std::f64::consts::FRAC_PI_4, ..TextStyle::default() },
                )
            })
        })
        .collect()
}

pub(crate) fn text_style_flags() -> Vec<Case> {
    (0..16)
        .map(|bits| {
            style_case(
                &format!("style/flags={bits}"),
                TextStyle {
                    backwards: bits & 1 != 0,
                    upside_down: bits & 2 != 0,
                    vertical: bits & 4 != 0,
                    annotative: bits & 8 != 0,
                    ..TextStyle::default()
                },
            )
        })
        .collect()
}

pub(crate) fn text_style_fonts() -> Vec<Case> {
    // Names only; no font files are read or required.
    [("CADCraft Stroke", ""), ("SyntheticSans.ttf", ""), ("synthetic.shx", "synthetic-big.shx")]
        .map(|(font, big_font)| {
            style_case(
                &format!("style/font={font}/big={big_font}"),
                TextStyle { font: font.into(), big_font: big_font.into(), ..TextStyle::default() },
            )
        })
        .into()
}

pub(crate) fn complex_linetypes(shape: bool) -> Vec<Case> {
    [false, true]
        .map(|absolute| {
            let mut case =
                common_case(&format!("linetype/shape={shape}/absolute={absolute}"), Common { linetype: "Dash_test".into(), ..Common::default() });
            if shape {
                case.drawing.text_styles.iter_mut().find(|s| s.name == "Notes").unwrap().font = "synthetic.shx".into();
            }
            let l = case.drawing.linetypes.iter_mut().find(|l| l.name == "Dash_test").unwrap();
            l.pattern.push(DashElement {
                text: (!shape).then(|| "GAS Ø".into()),
                shape: shape.then_some(7),
                style: Some("Notes".into()),
                scale: 0.75,
                rotation: std::f64::consts::FRAC_PI_2,
                offset: Vec2::new(0.125, -0.25),
                absolute,
                ..DashElement::dash(-1.0)
            });
            case
        })
        .into()
}

fn xdata_case(name: &str, tags: Vec<(i32, String)>) -> Case {
    common_case(name, Common { xdata: tags.into_iter().map(|(code, value)| RawTag { code, value }).collect(), ..Common::default() })
}

pub(crate) fn xdata_lists() -> Vec<Case> {
    vec![xdata_case(
        "xdata/nested-and-multiple-apps",
        [
            (1001, "ROUNDTRIP_A"),
            (1002, "{"),
            (1000, "outer"),
            (1002, "{"),
            (1070, "-32768"),
            (1002, "}"),
            (1071, "2147483647"),
            (1002, "}"),
            (1001, "ROUNDTRIP_B"),
            (1000, ""),
            (1000, "repeated Ø"),
            (1004, "00FF80"),
            (1041, "0.125"),
            (1042, "2.75"),
        ]
        .map(|(c, v)| (c, v.into()))
        .into(),
    )]
}

pub(crate) fn xdata_points() -> Vec<Case> {
    (1010..=1013)
        .map(|code| {
            xdata_case(
                &format!("xdata/point={code}"),
                vec![(1001, "ROUNDTRIP_POINTS".into()), (code, "1.25".into()), (code + 10, "-2.5".into()), (code + 20, "3.75".into())],
            )
        })
        .collect()
}

pub(crate) fn xdata_handles() -> Vec<Case> {
    let mut case = common_case("xdata/handle-reference", Common::default());
    let handles: Vec<_> = case.drawing.model.iter().map(|e| e.handle).collect();
    for &handle in &handles {
        case.drawing.model.modify(handle, |e| {
            e.common.xdata =
                vec![RawTag { code: 1001, value: "ROUNDTRIP_HANDLES".into() }, RawTag { code: 1005, value: handles.last().unwrap().hex() }]
        });
    }
    vec![case]
}

pub(crate) fn handles_and_order() -> Vec<Case> {
    [0x1_0000, 0x1_0000_0000]
        .map(|base| {
            let mut drawing = drawing();
            for index in [5, 1, 9, 3, 2, 8, 4, 7, 6] {
                let kind = if index % 2 == 0 {
                    EntityKind::Text(Text { value: format!("entity-{index}"), ..text() })
                } else {
                    EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(index as f64, 2.0, 3.0) })
                };
                drawing.model.push(Entity::new(Handle(base + index * 17), kind));
            }
            drawing.handseed = base + 0x1000;
            Case { name: format!("handles/sparse/base={base:X}"), drawing }
        })
        .into()
}

pub(crate) fn degenerate_geometry() -> Vec<Case> {
    [Vec3::ZERO, Vec3::new(1e9, -1e9, 1e-9)]
        .map(|p| entity_case(&format!("line/coincident={p:?}"), Common::default(), EntityKind::Line(Line { a: p, b: p })))
        .into()
}

pub(crate) fn text_interactions() -> Vec<Case> {
    let mut cases = Vec::new();
    for rotation in [-0.7, 0.0, 5.9] {
        for width_factor in [0.125, 2.75] {
            for oblique in [-0.3, 0.4] {
                cases.push(entity_case(
                    &format!("text/product/{rotation}/{width_factor}/{oblique}"),
                    Common::default(),
                    EntityKind::Text(Text {
                        rotation,
                        width_factor,
                        oblique,
                        height: 0.125,
                        style: "Notes".into(),
                        value: "  Ø 图 😀  ".into(),
                        ..text()
                    }),
                ));
            }
        }
    }
    cases
}

pub(crate) fn long_text() -> Vec<Case> {
    [254, 255, 256, 1024]
        .map(|length| {
            entity_case(&format!("text/length={length}"), Common::default(), EntityKind::Text(Text { value: "Ø".repeat(length), ..text() }))
        })
        .into()
}
