use super::args::{McpTarget, parse_mcp_args};
use super::error::{CliError, Result};

pub(crate) fn mcp(args: &[String]) -> Result {
    let backend: Box<dyn cadcraft_mcp::Backend> = match parse_mcp_args(args)? {
        McpTarget::Connect { addr, lazy: true, launch } => Box::new(cadcraft_mcp::Remote::lazy(&addr, launch)),
        McpTarget::Connect { addr, .. } => match cadcraft_mcp::Remote::connect(&addr) {
            Ok(r) => Box::new(r),
            Err(source) => return Err(CliError::McpConnect { addr, source }),
        },
        McpTarget::Headless => Box::new(cadcraft_mcp::Headless::default()),
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    cadcraft_mcp::Server::new(backend).serve(stdin.lock(), stdout.lock())?;
    Ok(())
}
