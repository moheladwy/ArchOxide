mod app;
mod appimage;
mod config;
mod desktop_apps;
mod flatpak;
mod fonts;
mod input;
mod package_details;
mod packages;
mod picker;
mod pty;
mod reads;
mod source_views;
mod terminal;
mod text_input;
mod themes;
mod transaction_terminal;
mod ui;

use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ArchOxide: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next();
    let launch_kitty = match (mode.as_deref(), args.next()) {
        (None, None) => false,
        (Some(mode), None) if mode == "--kitty" => true,
        (Some(mode), None) if mode == "--help" || mode == "-h" => {
            println!(
                "Usage: archoxide [--kitty]\n\n--kitty  Launch with the configured font in a dedicated Kitty window"
            );
            return Ok(());
        }
        _ => return Err("usage: archoxide [--kitty]".into()),
    };

    let path = config::config_path();
    let loaded = path
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|path| config::Config::load(path));
    let (config, warning) = match loaded {
        Ok(config) => (config, None),
        Err(error) => (config::Config::default(), Some(error)),
    };

    if launch_kitty {
        config.launch_kitty()
    } else {
        terminal::run(app::App::new(config, path.ok(), warning)).map_err(|error| error.to_string())
    }
}
