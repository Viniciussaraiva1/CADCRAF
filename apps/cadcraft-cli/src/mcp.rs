use super::args::{McpTarget, parse_mcp_args};

pub(crate) fn mcp(args: &[String]) -> Result<(), String> {
    let backend: Box<dyn cadcraft_mcp::Backend> = match parse_mcp_args(args)? {
        McpTarget::Connect { addr, lazy: true, launch } => Box::new(cadcraft_mcp::Remote::lazy(&addr, launch)),
        McpTarget::Connect { addr, .. } => match cadcraft_mcp::Remote::connect(&addr) {
            Ok(r) => Box::new(r),
            Err(e) => return Err(format!("cadcraft-cli: cannot connect to {addr}: {e}")),
        },
        McpTarget::Headless => Box::new(cadcraft_mcp::Headless::default()),
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    cadcraft_mcp::Server::new(backend).serve(stdin.lock(), stdout.lock()).map_err(|e| e.to_string())
}
