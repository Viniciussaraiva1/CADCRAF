use super::output::Streams;
use super::*;

pub(crate) fn info(path: &str, io: &mut Streams) -> Result {
    let mut s = Session::empty();
    open(&mut s, path)?;
    let v = s.execute("drawing.inspect", &json!({ "entities": false }))?;
    io.outln(format_args!("{}", serde_json::to_string_pretty(&v).unwrap_or_default()));
    Ok(())
}
