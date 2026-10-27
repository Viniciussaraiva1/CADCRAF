use crate::args::{ArgsError, WindowError};
use crate::output::OutputError;
use crate::perf::PerfError;
use crate::run::RunError;

/// Why a subcommand failed: printed as `error: {e}`, exit 1.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    #[error(transparent)]
    Engine(#[from] cadcraft_engine::EngineError),
    #[error(transparent)]
    Doc(#[from] cadcraft_engine::doc::DocError),
    #[error(transparent)]
    Format(#[from] cadcraft_io::IoError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Window(#[from] WindowError),
    #[error(transparent)]
    Args(#[from] ArgsError),
    #[error(transparent)]
    Run(#[from] RunError),
    #[error(transparent)]
    Output(#[from] OutputError),
    #[error(transparent)]
    Perf(#[from] PerfError),
    #[error("unknown sample `{0}` (bracket, floorplan)")]
    UnknownSample(String),
    #[error("cannot connect to {addr}: {source}")]
    McpConnect { addr: String, source: std::io::Error },
}

pub(crate) type Result<T = (), E = CliError> = std::result::Result<T, E>;
