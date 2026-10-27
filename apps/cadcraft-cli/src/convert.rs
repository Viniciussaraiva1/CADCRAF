use super::args::{Framing, USAGE, check_framed_output, framing_option};
use super::output::{Streams, export_framed, export_window, skipped_message, undrawn_message};
use super::*;

pub(crate) fn convert(args: &[String], io: &mut Streams) -> Result {
    let mut framing = Framing::Auto;
    let mut files = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if !framing_option(a, &mut it, &mut framing)? {
            files.push(a);
        }
    }
    let [i, o] = files.as_slice() else { return Err(CliError::Usage(USAGE.into())) };
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
