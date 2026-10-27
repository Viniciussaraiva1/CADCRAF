//! DWG must preserve the placement and edits of TEXT whose ordinary alignment anchor is
//! implicit. Aligned/Fit without a second endpoint must fail explicitly, not gain a new
//! length constraint. DXF keeps its existing optional-point representation.

use cadcraft_doc::{Attrib, Block, Common, Drawing, Entity, EntityKind, HAlign, Insert, Space, Text, VAlign};
use cadcraft_geom::{Mat3, Vec2, Vec3};
use cadcraft_io::{SaveOptions, Version};
use std::sync::Arc;

fn text(h: HAlign, v: VAlign) -> Text {
    Text {
        insert: Vec3::new(12.0, 7.0, 5.0),
        align_pt: None,
        height: 3.0,
        value: "AB".into(),
        rotation: 0.7,
        width_factor: 1.25,
        oblique: -0.2,
        style: "Standard".into(),
        halign: h,
        valign: v,
    }
}

fn drawing(t: Text) -> Drawing {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Text(t)).unwrap();
    d
}

fn first_text(d: &Drawing) -> &Text {
    let EntityKind::Text(t) = &d.model.iter().next().unwrap().kind else { panic!("TEXT") };
    t
}

fn reopen(d: &Drawing, name: &str, version: Version, binary: bool) -> Drawing {
    let bytes = cadcraft_io::write_with(d, name, &SaveOptions { version: Some(version), binary }).unwrap();
    let back = cadcraft_io::read(&bytes, name).unwrap();
    assert_eq!(cadcraft_io::save_version(&back), version);
    assert!(back.read_notes.is_empty(), "{:?}", back.read_notes);
    back
}

fn same_geometry(a: &Drawing, b: &Drawing) {
    let a = cadcraft_render::build(a, &Space::Model, &Default::default());
    let b = cadcraft_render::build(b, &Space::Model, &Default::default());
    assert!(!a.verts.is_empty());
    assert_eq!(a.prims.len(), b.prims.len());
    for (pa, pb) in a.prims.iter().zip(&b.prims) {
        assert_eq!(pa.kind, pb.kind);
        assert_eq!(a.points(pa).len(), b.points(pb).len());
        for (a, b) in a.points(pa).iter().zip(b.points(pb)) {
            // Wire angle conversions round slightly; the fixed fixture is only tens of units.
            assert!(a.dist(*b) < 1e-10, "vertex moved: {a:?} -> {b:?}");
        }
    }
}

fn edit(d: &mut Drawing) {
    let handle = d.model.iter().next().unwrap().handle;
    d.model.modify(handle, |e| {
        e.kind.transform(&Mat3::translate(Vec2::new(4.0, -2.0)));
        let EntityKind::Text(t) = &mut e.kind else { panic!("TEXT") };
        t.value = "AB AB AB".into();
    });
}

#[test]
fn dwg_keeps_implicit_anchors_geometry_and_value_edits() {
    for h in [HAlign::Left, HAlign::Center, HAlign::Right, HAlign::Middle] {
        for v in [VAlign::Baseline, VAlign::Bottom, VAlign::Middle, VAlign::Top] {
            if h == HAlign::Middle && v != VAlign::Baseline {
                continue;
            }
            for flags in 0..4 {
                let mut source = drawing(text(h, v));
                source.text_styles[0].backwards = flags & 1 != 0;
                source.text_styles[0].upside_down = flags & 2 != 0;
                let mut edited_source = source.clone();
                edit(&mut edited_source);
                for version in Version::ALL {
                    let mut current = source.clone();
                    for _ in 0..2 {
                        current = reopen(&current, "text.dwg", version, false);
                        let back = first_text(&current);
                        assert_eq!((back.halign, back.valign), (h, v));
                        assert_eq!(back.justify_point(), first_text(&source).justify_point());
                        assert_eq!(back.align_pt, if h == HAlign::Left && v == VAlign::Baseline { None } else { Some(first_text(&source).insert) });
                        same_geometry(&source, &current);
                        let mut edited = current.clone();
                        edit(&mut edited);
                        same_geometry(&edited_source, &edited);
                        let edited_back = reopen(&edited, "edited.dwg", version, false);
                        assert_eq!(first_text(&edited_back).value, "AB AB AB");
                        assert_eq!(first_text(&edited_back).justify_point(), first_text(&edited_source).justify_point());
                        same_geometry(&edited_source, &edited_back);
                    }
                }
                assert_eq!(first_text(&source).align_pt, None, "saving does not mutate the drawing");
            }
        }
    }
}

#[test]
fn dwg_refuses_missing_two_point_alignment_but_keeps_explicit_endpoints() {
    for h in [HAlign::Aligned, HAlign::Fit] {
        let source = drawing(text(h, VAlign::Baseline));
        for version in Version::ALL {
            let Err(error) = cadcraft_io::write_with(&source, "text.dwg", &SaveOptions { version: Some(version), binary: false }) else {
                panic!("DWG must refuse {h:?} without its second endpoint");
            };
            assert!(error.to_string().contains("second alignment point"), "{error}");
            let mut explicit = text(h, VAlign::Baseline);
            explicit.align_pt = Some(Vec3::new(18.0, 7.0, 5.0));
            let explicit = drawing(explicit);
            let back = reopen(&explicit, "text.dwg", version, false);
            assert_eq!(first_text(&back).align_pt, first_text(&explicit).align_pt);
            same_geometry(&explicit, &back);
        }
    }
}

#[test]
fn dwg_keeps_authored_ordinary_alignment_points() {
    for (h, v) in
        [(HAlign::Left, VAlign::Bottom), (HAlign::Center, VAlign::Baseline), (HAlign::Right, VAlign::Top), (HAlign::Middle, VAlign::Baseline)]
    {
        let mut t = text(h, v);
        t.align_pt = Some(Vec3::new(20.0, 8.0, 5.0));
        let source = drawing(t);
        for version in Version::ALL {
            let back = reopen(&source, "explicit.dwg", version, false);
            assert_eq!(first_text(&back).align_pt, first_text(&source).align_pt);
            same_geometry(&source, &back);
        }
    }
}

#[test]
fn dxf_keeps_absent_alignment_points() {
    for h in [HAlign::Left, HAlign::Center, HAlign::Right, HAlign::Middle, HAlign::Aligned, HAlign::Fit] {
        let source = drawing(text(h, if h == HAlign::Left { VAlign::Bottom } else { VAlign::Baseline }));
        for version in Version::ALL {
            for binary in [false, true] {
                let back = reopen(&source, "text.dxf", version, binary);
                let actual = first_text(&back);
                let mut expected = first_text(&source).clone();
                // Only degree/radian conversion roundoff is permitted, not point synthesis.
                assert!((actual.rotation - expected.rotation).abs() <= 8.0 * f64::EPSILON);
                assert!((actual.oblique - expected.oblique).abs() <= 8.0 * f64::EPSILON);
                expected.rotation = actual.rotation;
                expected.oblique = actual.oblique;
                assert_eq!(actual, &expected);
                same_geometry(&source, &back);
            }
        }
    }
}

#[test]
fn entity_inspection_keeps_the_authored_optional_point() {
    let source = drawing(text(HAlign::Right, VAlign::Top));
    let entity = source.model.iter().next().unwrap();
    let (kind, tags) = cadcraft_io::entity_groups(&source, entity);
    assert_eq!(kind, "TEXT");
    assert!(!tags.iter().any(|tag| matches!(tag.code, 11 | 21 | 31)));
    assert_eq!(tags.iter().find(|tag| tag.code == 10).unwrap().f64(), 12.0);
    assert_eq!(tags.iter().find(|tag| tag.code == 20).unwrap().f64(), 7.0);
    let back = reopen(&source, "text.dwg", Version::R2018, false);
    assert_eq!(first_text(&back).align_pt, Some(first_text(&source).insert));
    assert_eq!(cadcraft_io::entity_groups(&source, entity), (kind, tags));
}

fn attribute() -> Attrib {
    Attrib {
        tag: "ID".into(),
        // Nonbaseline ATTRIB reading currently drops vertical justification; isolate anchor export here.
        text: text(HAlign::Center, VAlign::Baseline),
        invisible: false,
        constant: false,
        prompt: String::new(),
        props: Default::default(),
        multiline: None,
        modes: Default::default(),
    }
}

fn texts(d: &Drawing) -> Vec<&Text> {
    d.model
        .iter()
        .chain(d.layouts.iter().flat_map(|l| l.entities.iter()))
        .chain(d.blocks.values().flat_map(|b| b.entities.iter()))
        .flat_map(|e| match &e.kind {
            EntityKind::Text(t) => vec![t],
            EntityKind::AttDef(a) => vec![&a.text],
            EntityKind::Insert(i) => i.attribs.iter().map(|a| &a.text).collect(),
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn invented_endpoint_changes_future_value_edits() {
    for h in [HAlign::Aligned, HAlign::Fit] {
        let mut t = text(h, VAlign::Baseline);
        let mut implicit = drawing(t.clone());
        let width = cadcraft_fonts::text_width(&cadcraft_fonts::TextFont::Stroke, &t.value, t.height, t.width_factor);
        t.align_pt = Some(t.insert + Vec3::new(width * t.rotation.cos(), width * t.rotation.sin(), 0.0));
        let mut invented = reopen(&drawing(t), "invented.dwg", Version::R2018, false);
        same_geometry(&implicit, &invented);
        edit(&mut implicit);
        edit(&mut invented);
        let a = cadcraft_render::build(&implicit, &Space::Model, &Default::default());
        let b = cadcraft_render::build(&invented, &Space::Model, &Default::default());
        assert_eq!(a.verts.len(), b.verts.len());
        let max_shift = a.verts.iter().zip(&b.verts).map(|(a, b)| a.dist(*b)).fold(0.0_f64, f64::max);
        assert!(max_shift > 1.0, "an invented endpoint silently introduces a length constraint");
    }
}

fn insert(block: &str) -> Insert {
    Insert {
        block: block.into(),
        insert: Vec3::ZERO,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: vec![],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    }
}

#[test]
fn xref_contents_do_not_block_host_export() {
    let mut d = drawing(text(HAlign::Left, VAlign::Baseline));
    let mut external = Block::new("External");
    external.xref_path = Some("external.dxf".into());
    external.entities.push(Entity::new(d.new_handle(), EntityKind::Text(text(HAlign::Fit, VAlign::Baseline))));
    d.blocks.insert(external.name.clone(), Arc::new(external));
    let mut dependent = Block::new("External|Child");
    dependent.entities.push(Entity::new(d.new_handle(), EntityKind::Text(text(HAlign::Aligned, VAlign::Baseline))));
    d.blocks.insert(dependent.name.clone(), Arc::new(dependent));
    d.add(&Space::Model, Common::default(), EntityKind::Insert(insert("External"))).unwrap();
    for version in Version::ALL {
        let back = reopen(&d, "host.dwg", version, false);
        assert!(back.block("External").unwrap().entities.is_empty());
        assert!(back.block("External|Child").is_none());
        // The same contents are rejected if they belong to a stored local block.
        let mut local = d.clone();
        Arc::make_mut(local.blocks.get_mut("External").unwrap()).xref_path = None;
        assert!(cadcraft_io::write_with(&local, "local.dwg", &SaveOptions { version: Some(version), binary: false }).is_err());
    }
    assert_eq!(d.block("External").unwrap().entities.len(), 1);
    assert!(texts(&d).iter().all(|t| t.align_pt.is_none()));
}

#[test]
fn uncommitted_refedit_objects_do_not_block_export() {
    let mut d = Drawing::new_metric();
    let mut block = Block::new("Tag");
    let original = d.new_handle();
    block.entities.push(Entity::new(original, EntityKind::Text(text(HAlign::Left, VAlign::Bottom))));
    d.blocks.insert(block.name.clone(), Arc::new(block));
    let handle = d.add(&Space::Model, Common { visible: false, ..Default::default() }, EntityKind::Insert(insert("Tag"))).unwrap();
    let copy = d.add(&Space::Model, Common::default(), EntityKind::Text(text(HAlign::Fit, VAlign::Baseline))).unwrap();
    d.refedit = Some(cadcraft_doc::RefEdit {
        block: "Tag".into(),
        insert: handle,
        insert_was_visible: true,
        space: Space::Model,
        to_world: Mat3::IDENTITY,
        items: vec![cadcraft_doc::WorkItem { copy, source: Some(original) }],
        ghosts: vec![],
        released: vec![],
    });
    for version in Version::ALL {
        let back = reopen(&d, "during-edit.dwg", version, false);
        assert!(back.model.get(copy).is_none());
        assert!(back.model.get(handle).unwrap().common.visible);
        same_geometry(&d.committed(), &back);
        // Objects added from the host (source=None) survive discard and must be validated.
        let mut retained = d.clone();
        retained.refedit.as_mut().unwrap().items[0].source = None;
        assert!(cadcraft_io::write_with(&retained, "retained.dwg", &SaveOptions { version: Some(version), binary: false }).is_err());
    }
    assert!(d.refedit.is_some());
    assert!(d.model.get(copy).is_some());
    assert!(!d.model.get(handle).unwrap().common.visible);
    assert!(texts(&d).iter().all(|t| t.align_pt.is_none()));
}

#[test]
fn emitted_text_records_reject_missing_endpoints_and_nonfinite_implicit_anchors() {
    let mut cases = vec![text(HAlign::Aligned, VAlign::Baseline), text(HAlign::Fit, VAlign::Baseline)];
    for coordinate in 0..3 {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut t = text(HAlign::Right, VAlign::Top);
            match coordinate {
                0 => t.insert.x = value,
                1 => t.insert.y = value,
                _ => t.insert.z = value,
            }
            cases.push(t);
        }
    }
    for t in cases {
        for location in 0..5 {
            let mut d = Drawing::new_metric();
            let mut a = attribute();
            a.text = t.clone();
            match location {
                0 => {
                    d.add(&Space::Model, Common::default(), EntityKind::Text(t.clone())).unwrap();
                }
                1 => {
                    d.add(&Space::Paper("Layout1".into()), Common::default(), EntityKind::Text(t.clone())).unwrap();
                }
                2 | 3 => {
                    let mut block = Block::new("Tag");
                    let kind = if location == 2 { EntityKind::Text(t.clone()) } else { EntityKind::AttDef(a) };
                    block.entities.push(Entity::new(d.new_handle(), kind));
                    d.blocks.insert(block.name.clone(), Arc::new(block));
                }
                _ => {
                    d.blocks.insert("Tag".into(), Arc::new(Block::new("Tag")));
                    let mut i = insert("Tag");
                    i.attribs.push(a);
                    d.add(&Space::Model, Common::default(), EntityKind::Insert(i)).unwrap();
                }
            }
            let before = d.clone();
            for version in Version::ALL {
                let Err(error) = cadcraft_io::write_with(&d, "invalid.dwg", &SaveOptions { version: Some(version), binary: false }) else {
                    panic!("location {location} must reject {t:?}");
                };
                let reason = if matches!(t.halign, HAlign::Aligned | HAlign::Fit) { "second alignment point" } else { "must be finite" };
                assert!(error.to_string().contains(reason), "{error}");
                assert!(d.model.same_as(&before.model), "failed export must not mutate the source");
                assert!(texts(&d).iter().all(|t| t.align_pt.is_none()));
            }
        }
    }
}

#[test]
fn dwg_resolves_text_records_in_blocks_layouts_and_attributes() {
    let mut d = drawing(text(HAlign::Left, VAlign::Bottom));
    d.add(&Space::Paper("Layout1".into()), Common::default(), EntityKind::Text(text(HAlign::Left, VAlign::Bottom))).unwrap();
    let mut block = Block::new("Anchors");
    block.entities.push(Entity::new(d.new_handle(), EntityKind::Text(text(HAlign::Left, VAlign::Bottom))));
    block.entities.push(Entity::new(d.new_handle(), EntityKind::AttDef(attribute())));
    let insert = insert("Anchors");
    // Stored records, not expanded references: even a cycle must terminate.
    block.entities.push(Entity::new(d.new_handle(), EntityKind::Insert(insert.clone())));
    d.blocks.insert(block.name.clone(), Arc::new(block));
    d.add(&Space::Model, Common::default(), EntityKind::Insert(Insert { attribs: vec![attribute()], ..insert })).unwrap();
    for version in Version::ALL {
        let back = reopen(&d, "locations.dwg", version, false);
        let records = texts(&back);
        assert_eq!(records.len(), 5);
        for t in records {
            assert_eq!(t.align_pt, Some(Vec3::new(12.0, 7.0, 5.0)));
            assert_eq!(t.justify_point(), Vec2::new(12.0, 7.0));
        }
    }
    assert!(texts(&d).iter().all(|t| t.align_pt.is_none()));
}
