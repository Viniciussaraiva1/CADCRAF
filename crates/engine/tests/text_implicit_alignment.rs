//! Making an implicit TEXT anchor explicit in DWG must preserve grip editing, property edits,
//! and undo/redo. These exercise the engine API and interactive prompt path, not OS mouse input.

use cadcraft_engine::Session;
use cadcraft_engine::doc::{Common, Drawing, EntityKind, HAlign, Space, Text, VAlign};
use cadcraft_engine::geom::{Vec2, Vec3};
use cadcraft_engine::grips::GripMode;
use cadcraft_io::{SaveOptions, Version};
use serde_json::json;

fn reopened(d: &Drawing, version: Version) -> Drawing {
    let bytes = cadcraft_io::write_with(d, "text.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
    cadcraft_io::read(&bytes, "text.dwg").unwrap()
}

fn geometry(d: &Drawing) -> Vec<Vec2> {
    let list = cadcraft_engine::render::build(d, &Space::Model, &Default::default());
    assert!(!list.verts.is_empty());
    list.verts
}

fn same_geometry(a: &Drawing, b: &Drawing) {
    let a = geometry(a);
    let b = geometry(b);
    assert_eq!(a.len(), b.len());
    assert!(a.iter().zip(b).all(|(a, b)| a.dist(b) < 1e-10));
}

#[test]
fn rejected_dwg_save_keeps_existing_file_and_unsaved_drawing() {
    use cadcraft_engine::cmd::file::{IoHooks, set_io};
    use std::io::Write;

    set_io(IoHooks {
        read: |bytes, name| cadcraft_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: None,
    });
    let mut s = Session::new();
    let saved = s.state().unwrap().saved.clone();
    let previous_bytes = cadcraft_io::write(s.doc().unwrap(), "previous.dwg").unwrap();
    let path = std::env::temp_dir().join(format!("cadcraft-text-rejected-save-{}.dwg", std::process::id()));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
    file.write_all(&previous_bytes).unwrap();
    drop(file);
    s.execute("text", &json!({"at": [12, 7], "text": "AB", "height": 3})).unwrap();
    let handle = s.doc().unwrap().model.iter().next().unwrap().handle;
    s.doc_mut()
        .unwrap()
        .modify_entity(handle, |e| {
            let EntityKind::Text(t) = &mut e.kind else { panic!("TEXT") };
            t.halign = HAlign::Fit;
            t.align_pt = None;
        })
        .unwrap();
    let before = s.doc().unwrap().clone();
    let result = s.execute("saveas", &json!({"path": path.to_str().unwrap()}));
    let actual_bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let error = result.unwrap_err();
    assert!(error.to_string().contains("second alignment point"), "{error}");
    assert_eq!(actual_bytes, previous_bytes);
    assert_eq!(s.doc().unwrap().model, before.model);
    assert!(s.state().unwrap().path.is_none());
    assert_eq!(s.state().unwrap().saved.model, saved.model);
}

#[test]
fn reopened_implicit_text_keeps_grips_property_edits_and_undo() {
    for (h, v) in
        [(HAlign::Left, VAlign::Bottom), (HAlign::Center, VAlign::Baseline), (HAlign::Right, VAlign::Top), (HAlign::Middle, VAlign::Baseline)]
    {
        let mut source = Drawing::new_metric();
        let handle = source
            .add(
                &Space::Model,
                Common::default(),
                EntityKind::Text(Text {
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
                }),
            )
            .unwrap();
        for version in Version::ALL {
            for interactive in [false, true] {
                let mut edited = Vec::new();
                for d in [source.clone(), reopened(&source, version)] {
                    let before = d.clone();
                    let mut s = Session::new();
                    *s.doc_mut().unwrap() = d;
                    assert_eq!(s.grips_of(handle).unwrap(), vec![Vec2::new(12.0, 7.0)]);
                    if interactive {
                        s.set_selection(vec![handle]);
                        s.start_grip_edit(&[(handle, 0)]).unwrap();
                        s.cmdline("16,5").unwrap();
                        assert!(s.running.is_none(), "{}", s.prompt_text());
                    } else {
                        s.grip_edit(handle, 0, Vec2::new(16.0, 5.0), GripMode::Stretch).unwrap();
                    }
                    assert_eq!(s.grips_of(handle).unwrap(), vec![Vec2::new(16.0, 5.0)]);
                    let after = s.doc().unwrap().clone();
                    assert!(geometry(&before).iter().zip(geometry(&after)).all(|(a, b)| (*a + Vec2::new(4.0, -2.0)).dist(b) < 1e-10));
                    s.execute("undo", &json!({})).unwrap();
                    assert_eq!(s.doc().unwrap().model, before.model);
                    s.execute("redo", &json!({})).unwrap();
                    assert_eq!(s.doc().unwrap().model, after.model);
                    s.execute("properties.set", &json!({"handles": [handle.hex()], "text": "AB AB AB", "height": 4.5})).unwrap();
                    assert_eq!(s.grips_of(handle).unwrap(), vec![Vec2::new(16.0, 5.0)]);
                    let EntityKind::Text(t) = &s.doc().unwrap().entity(handle).unwrap().kind else { panic!("TEXT") };
                    assert_eq!(t.value, "AB AB AB");
                    assert_eq!(t.height, 4.5);
                    let back = reopened(s.doc().unwrap(), version);
                    same_geometry(s.doc().unwrap(), &back);
                    edited.push(back);
                }
                same_geometry(&edited[0], &edited[1]);
            }
        }
    }
}
