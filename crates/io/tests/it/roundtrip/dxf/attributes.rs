//! Separate visible multiline-attribute geometry from fallback TEXT-field preservation.
//! Embedded MTEXT can keep drawing correctly even when the reader corrupts justification.
//! The strict pending check in `extended::multiline_attributes` must not be relaxed on that
//! basis (#2255). ODA's DWG specification §20.4.4 documents the R2018+ embedded-object
//! representation and its DXF group-101 mapping; older DXF wire conformance is not tested here.

use super::super::cases::blocks;
use cadcraft_doc::{Drawing, EntityKind, Space};
use cadcraft_io::{SaveOptions, Version};
use cadcraft_render::{DisplayList, Options, build};

fn geometry(drawing: &Drawing) -> DisplayList {
    build(drawing, &Space::Model, &Options::default())
}

fn same_geometry(expected: &DisplayList, actual: &DisplayList, context: &str) {
    assert!(!expected.prims.is_empty(), "{context}: nonempty fixture");
    assert_eq!(expected.prims.len(), actual.prims.len(), "{context}: primitive count");
    for (a, b) in expected.prims.iter().zip(&actual.prims) {
        assert_eq!(a.kind, b.kind, "{context}: primitive kind");
        assert_eq!(expected.points(a), actual.points(b), "{context}: world-space geometry");
    }
}

/// Downgrade the authored and reopened drawings independently. The single-line fallback must
/// not move just because a prior modern save changed its otherwise hidden TEXT fields.
pub(super) fn same_legacy_geometry(original: &Drawing, back: &Drawing, context: &str) {
    for version in [Version::R2000, Version::R2004] {
        for binary in [false, true] {
            let options = SaveOptions { version: Some(version), binary };
            let downgrade = |d| {
                let bytes = cadcraft_io::write_with(d, "legacy.dxf", &options).unwrap();
                cadcraft_io::read(&bytes, "legacy.dxf").unwrap()
            };
            same_geometry(
                &geometry(&downgrade(original)),
                &geometry(&downgrade(back)),
                &format!("{context} · downgrade {version} · binary={binary}"),
            );
        }
    }
}

fn edit_values(drawing: &mut Drawing) {
    let handles: Vec<_> = drawing.model.iter().map(|e| e.handle).collect();
    let mut changed = 0;
    for handle in handles {
        drawing.model.modify(handle, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                for a in &mut i.attribs {
                    if a.multiline.is_some() {
                        a.text.value = "edited {value}\nthird line".into();
                        changed += 1;
                    }
                }
            }
        });
    }
    assert!(changed > 0, "fixture must contain a visible multiline attribute");
}

#[test]
fn multiline_geometry_and_value_edits_survive_two_saves() {
    for case in blocks::multiline_attributes() {
        let expected = geometry(&case.drawing);
        let mut edited = case.drawing.clone();
        edit_values(&mut edited);
        let edited_expected = geometry(&edited);
        assert_ne!(expected.verts, edited_expected.verts, "value edit must actually change the drawn text");
        for binary in [false, true] {
            let options = SaveOptions { version: Some(Version::R2018), binary };
            let mut current = case.drawing.clone();
            for pass in 1..=2 {
                let context = format!("{} · R2018 · binary={binary} · pass={pass}", case.name);
                let bytes = cadcraft_io::write_with(&current, "attributes.dxf", &options).unwrap();
                current = cadcraft_io::read(&bytes, "attributes.dxf").unwrap();
                same_geometry(&expected, &geometry(&current), &context);
                let mut edited = current.clone();
                edit_values(&mut edited);
                same_geometry(&edited_expected, &geometry(&edited), &format!("{context} · edit after reopening"));
                let bytes = cadcraft_io::write_with(&edited, "attributes.dxf", &options).unwrap();
                let back = cadcraft_io::read(&bytes, "attributes.dxf").unwrap();
                same_geometry(&edited_expected, &geometry(&back), &format!("{context} · save after editing"));
            }
        }
    }
}
