//! Headless shell.
//!
//! Runs the same `Node` as the desktop app without a UI. Durable events and
//! live model deltas go to stdout as JSONL; human summaries and logs go to
//! stderr. This is the primary way features are exercised in CI.

mod cli;
mod commands;

use phi_i18n as i18n;

#[tokio::main]
async fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    // Resolve the language before parsing so even usage errors are localized.
    let language = cli::scan_lang(&raw);
    i18n::set_lang(i18n::resolve(language.as_deref(), None));
    let (global, command) = match cli::parse_cli(raw) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("[phi-headless] {error}");
            eprintln!();
            cli::print_help();
            std::process::exit(2);
        }
    };
    let code = match commands::execute(global, command).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("[phi-headless] error: {error}");
            2
        }
    };
    std::process::exit(code);
}
