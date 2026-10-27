//! Dynamic block purge preventers (FF1): acadrust 0.6.5's DXF writer leaves them out of the DXF
//! CADCraft reads, so the DWG import puts them back; a DWG save then keeps them, in their block
//! record's extension dictionary. Original synthetic source; the DWG oracle shares acadrust 0.6.5,
//! no AutoCAD validation is claimed.
#![cfg(not(target_arch = "wasm32"))]

use acadrust::classes::DxfClass;
use acadrust::entities::Line;
use acadrust::objects::{BlockRepresentationData, DynamicBlockData, DynamicBlockObject, ObjectType};
use acadrust::{CadDocument, DwgWriter, DxfVersion, EntityType};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const NAME: &str = "ACDB_DYNAMICBLOCKPURGEPREVENTER_VERSION";
const KEY: &str = "AcDbDynamicBlockRoundTripPurgePreventer";

/// AC1032 with a line and model space's extension dictionary holding a purge preventer.
fn source() -> TestResult<Vec<u8>> {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    doc.add_entity(EntityType::Line(Line::from_coords(0., 0., 0., 10., 0., 0.)))?;
    let mut class = DxfClass::new(NAME, "AcDbDynamicBlockPurgePreventer");
    class.application_name = "ObjectDBX Classes".into();
    doc.classes.add_or_update(class);
    let ms = doc.block_records.get("*Model_Space").ok_or("model space")?.handle;
    let xd = doc.ensure_extension_dictionary(ms);
    let h = doc.allocate_handle();
    let mut p = DynamicBlockObject::new(NAME, "AcDbDynamicBlockPurgePreventer");
    p.handle = h;
    p.owner = xd;
    p.reactors = vec![xd];
    p.data = DynamicBlockData::Representation(BlockRepresentationData { flags: 1, block: ms });
    doc.objects.insert(h, ObjectType::DynamicBlock(p));
    let Some(ObjectType::Dictionary(d)) = doc.objects.get_mut(&xd) else { return Err("extension dictionary".into()) };
    d.entries.push((KEY.into(), h));
    Ok(DwgWriter::write_to_vec(&doc)?)
}

/// The purge preventer in model space's extension dictionary: (flags, whether it names model space).
fn preventer(bytes: &[u8]) -> TestResult<Option<(i16, bool)>> {
    let out = acadrust::DwgReader::from_stream(std::io::Cursor::new(bytes)).read_with_stats()?;
    assert_eq!((out.stats.skipped_source_records, out.stats.recovered_errors), (0, 0));
    let doc = out.document;
    let ms = doc.block_records.get("*Model_Space").ok_or("model space")?.handle;
    let Some(xd) = doc.extension_dictionary_handle(ms) else { return Ok(None) };
    let Some(ObjectType::Dictionary(d)) = doc.objects.get(&xd) else { return Ok(None) };
    let Some((_, h)) = d.entries.iter().find(|(k, _)| k == KEY) else { return Ok(None) };
    Ok(match doc.objects.get(h) {
        Some(ObjectType::DynamicBlock(DynamicBlockObject { dxf_name, owner, data: DynamicBlockData::Representation(r), .. }))
            if dxf_name == NAME && *owner == xd =>
        {
            Some((r.flags, r.block == ms))
        }
        _ => None,
    })
}

/// The preventer (flags and block) survives two DWG saves through CADCraft.
#[test]
fn a_purge_preventer_survives_two_dwg_saves() -> TestResult {
    let mut bytes = source()?;
    assert_eq!(preventer(&bytes)?, Some((1, true)));
    for _ in 0..2 {
        let d = cadcraft_io::read(&bytes, "in.dwg")?;
        bytes = cadcraft_io::write_with(&d, "out.dwg", &cadcraft_io::SaveOptions::default())?;
        assert_eq!(preventer(&bytes)?, Some((1, true)));
    }
    Ok(())
}
