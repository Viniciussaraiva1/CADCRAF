use super::args::SampleArgs;
use super::error::{CliError, Result};
use super::output::{Streams, write_atomic};

pub(crate) fn sample(a: &SampleArgs, io: &mut Streams) -> Result {
    let out = &a.output;
    let d = match a.name.as_str() {
        "bracket" => cadcraft_engine::sample::bracket(),
        "floorplan" | "floor" => cadcraft_engine::sample::floor_plan(),
        other => return Err(CliError::UnknownSample(other.to_string())),
    };
    let bytes = cadcraft_io::write(&d, out)?;
    write_atomic(out, &bytes)?;
    io.errln(format_args!("wrote {out} ({} bytes)", bytes.len()));
    Ok(())
}
