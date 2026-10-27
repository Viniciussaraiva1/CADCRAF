use super::args::{ConvertArgs, check_framed_output};
use super::output::{Streams, export_framed, export_window, skipped_message, undrawn_message};
use super::*;

pub(crate) fn convert(a: &ConvertArgs, io: &mut Streams) -> Result {
    let (i, o) = (&a.input, &a.output);
    let framing = a.framing.framing()?;
    check_framed_output(framing, Some(o))?;
    let mut s = Session::empty();
    open(&mut s, i)?;
    if let Some(m) = skipped_message(s.doc()?, i) {
        io.errln(format_args!("{m}"));
    }
    if cadcraft_io::is_image(o)
        && let Some(m) = undrawn_message(s.doc()?, i)
    {
        io.errln(format_args!("{m}"));
    }
    let window = export_window(&s, framing, None)?;
    export_framed(&s, o, window, io)
}
