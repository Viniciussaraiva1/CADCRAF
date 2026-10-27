# cadcraft-cli

`cadcraft-cli` is headless CADCraft: it reads, converts and exports drawings, runs scripts and
commands, prints the command catalog and serves MCP. It follows
[RFC-0012: Command-line conventions](https://github.com/storytold/craft-contrib/pull/18).

`cadcraft-cli --help` and `cadcraft-cli help <subcommand>` (or `<subcommand> --help`) are the
reference for every subcommand, flag and value; this page doesn't repeat them. Examples are in the
[README](../README.md#drive-it-from-agents-mcp-and-the-cli), MCP setup in [mcp.md](mcp.md),
scripts and the JSON command forms in [customization.md](customization.md).

## Conventions

| | |
|---|---|
| Exit codes | `0` success, `1` the command failed, `2` usage error. |
| Streams | Results on stdout (JSON where a program reads them). Errors (`error: …`), warnings and `wrote …` lines on stderr. `mcp` speaks MCP on stdout and writes nothing else there. |
| Usage errors | An unknown or repeated flag, a missing or malformed value (`--window`, `--view`, `--cmd` JSON, a sample name), flags that conflict (`--window` with `--view`) or that need another (`run --window` without `--export`) stop before any work, with exit 2. `--script`, `--script-file`, `--cmd` and `run`'s files repeat and run in command-line order. |
| Values starting with `-` | Written with `=`: `--window=-10,-10,10,10`, `--export=-plan.png`. |
| Paths | Read as given, including names that aren't valid Unicode. The engine opens and writes files by text name, so such a name is reported as an error (exit 1) where a file is opened or written. |
| Completions and man pages | The Linux packages install them. To generate them yourself: `cargo run -p cadcraft-cli --example man-and-completions -- <out-dir>`. |

## Environment variables

`cadcraft-cli` reads none. The desktop app's are listed by `cadcraft --help`; its one flag
fallback is `CADCRAFT_CONTROL_PORT` for `--control`. An empty value counts as unset, the flag
wins over the variable, and a bad value is a usage error that names the variable.

## Not followed yet

- **No variable for `mcp --connect`.** Photocraft's `mcp --bridge` falls back to its
  `*_CONTROL_PORT` variable. Here an exported `CADCRAFT_CONTROL_PORT` would silently switch `mcp`
  from a headless session to the running app, so `--connect` stays a flag only.
- **The desktop app on Windows** is built without a console, so a usage error in its launch flags
  isn't visible there; the app exits with code 2.
- **The `string-error` ast-grep rule** covers the CLI code only. The engine's file hooks
  (`cadcraft_engine::cmd::file::IoHooks`) still return `Result<_, String>`, so `install_io` maps
  errors to strings at that boundary.
- **No AGENTS.md line for the ast-grep scan.** RFC-0012 asks an app's AGENTS.md to have coding
  agents run the scan before pushing (`ast-grep scan -c lint/ast-grep/sgconfig.yml`); CI runs it
  on every PR that touches the CLI code.
