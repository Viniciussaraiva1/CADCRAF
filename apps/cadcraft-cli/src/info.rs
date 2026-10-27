use super::args::InfoArgs;
use super::output::Streams;
use super::*;

pub(crate) fn info(a: &InfoArgs, io: &mut Streams) -> Result {
    let mut s = Session::empty();
    open(&mut s, &a.file)?;
    let v = s.execute("drawing.inspect", &json!({ "entities": false }))?;
    io.json(&v, a.compact);
    Ok(())
}
