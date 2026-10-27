use super::args::CommandsArgs;
use super::output::Streams;
use super::*;

pub(crate) fn commands(a: &CommandsArgs, io: &mut Streams) -> Result {
    let s = Session::new();
    let f = a.filter.as_ref().map(|x| x.to_ascii_lowercase());
    let v: Vec<Value> = cadcraft_engine::command_specs()
        .iter()
        .filter(|c| f.as_ref().is_none_or(|f| c.id.contains(f.as_str()) || c.label.to_ascii_lowercase().contains(f.as_str())))
        .map(|c| serde_json::to_value(c.info(&s)).unwrap_or_default())
        .collect();
    io.outln(format_args!("{}", serde_json::to_string_pretty(&v).unwrap_or_default()));
    Ok(())
}
