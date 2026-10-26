//! DXF preservation through public Drawing → bytes → Drawing APIs, without disk or UI.
//! The first slice covers model-space LINE, CIRCLE, TEXT, all their fields, and referenced
//! layers/linetypes/text styles, not whole-drawing equivalence. Every case runs at every save
//! version in ASCII and binary, twice; neither pass may derive its expectation from writer output.
//!
//! Run: `cargo test -p cadcraft-io --test it roundtrip::dxf`.
//! Add `-- --ignored` to reproduce the known extrusion/ByBlock plot-style losses. Those tests
//! assert desired preservation, not today's wrong result; remove `ignore` when fixing them.
//! New entity families need an explicit comparator and independent input/output examples.

mod common;
mod conformance;
mod entities;

use super::compare;
use cadcraft_doc::Drawing;
use cadcraft_io::{SaveOptions, Version, read, save_version, write_with};

fn check(case: &str, original: &Drawing) {
    for version in Version::ALL {
        for binary in [false, true] {
            let options = SaveOptions { version: Some(version), binary };
            let mut current = original.clone();
            for pass in 1..=2 {
                let context = format!("{case} · {version} · binary={binary} · pass={pass}");
                let bytes = write_with(&current, "memory.dxf", &options).unwrap_or_else(|e| panic!("{context}: write: {e}"));
                assert_eq!(cadcraft_dxf::is_binary(&bytes), binary, "{context}: encoding");
                let back = read(&bytes, "memory.dxf").unwrap_or_else(|e| panic!("{context}: read: {e}"));
                assert_eq!(save_version(&back), version, "{context}: version");
                assert!(back.read_notes.is_empty(), "{context}: read notes: {:?}", back.read_notes);
                assert!(back.skipped_records.is_empty(), "{context}: skipped records: {:?}", back.skipped_records);
                compare::model(original, &back, &context);
                if pass == 2 {
                    compare::model(&current, &back, &format!("{context} · resave stability"));
                }
                current = back;
            }
        }
    }
}
