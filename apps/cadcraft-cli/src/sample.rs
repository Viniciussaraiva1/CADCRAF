use super::args::{SampleArgs, SampleName};
use super::error::Result;
use super::output::{Streams, write_atomic};
use super::unicode;

pub(crate) fn sample(a: &SampleArgs, io: &mut Streams) -> Result {
    let out = unicode(&a.output)?;
    let d = match a.name {
        SampleName::Bracket => cadcraft_engine::sample::bracket(),
        SampleName::Floorplan => cadcraft_engine::sample::floor_plan(),
    };
    let bytes = cadcraft_io::write(&d, out)?;
    write_atomic(out, &bytes)?;
    io.errln(format_args!("wrote {out} ({} bytes)", bytes.len()));
    Ok(())
}
