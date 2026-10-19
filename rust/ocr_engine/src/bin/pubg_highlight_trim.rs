use clap::{error::ErrorKind, Parser};
use pubg_ocr_engine::{application, config::Config};
use std::process::ExitCode;
fn main() -> ExitCode {
    let c = match Config::try_parse() {
        Ok(c) => c,
        Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
            print!("{e}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprint!("{e}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = pubg_ocr_engine::cancellation::install_handler() {
        eprintln!("ERROR {e}");
        return ExitCode::from(1);
    }
    match application::run(c) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("ERROR {e}");
            ExitCode::from(1)
        }
    }
}
