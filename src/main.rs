mod app;
mod input;
mod terminal;
mod ui;

use std::process::ExitCode;

fn main() -> ExitCode {
    match terminal::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ArchOxide: {error}");
            ExitCode::FAILURE
        }
    }
}
