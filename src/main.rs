use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use hearth::app::App;
use hearth::cli::Cli;
use hearth::config::{self, Config};
use hearth::events::{self, TerminalEvents};
use hearth::scene::FRAME_RATE;
use hearth::scenes::typing;

fn main() {
    let cli = Cli::parse();
    let config_path = config::default_path();
    let config = match &config_path {
        Some(path) => Config::load(path).unwrap_or_else(|e| {
            eprintln!("hearth: {e}");
            std::process::exit(1);
        }),
        None => Config::default(),
    };
    let corpus = match cli.corpus(&config) {
        Ok(corpus) if corpus.is_empty() => {
            eprintln!("hearth: that repository has no text file to type");
            std::process::exit(1);
        }
        Ok(corpus) if cli.repo(&config).is_some() && !typing::has_snippet(&corpus) => {
            eprintln!("hearth: that repository has no function to type");
            std::process::exit(1);
        }
        Ok(corpus) => Arc::new(corpus),
        Err(e) => {
            eprintln!("hearth: {e}");
            std::process::exit(1);
        }
    };
    let mut app = cli.app(&config, corpus);
    if let Some(path) = config_path {
        app = app.saving_to(path);
    }
    if let Err(e) = run_tui(app) {
        eprintln!("hearth: {e}");
        std::process::exit(1);
    }
}

/// Runs the loop on the alternate screen and restores the terminal on every
/// exit, including the error one.
fn run_tui(mut app: App) -> std::io::Result<()> {
    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(e) => {
            ratatui::restore();
            return Err(std::io::Error::new(
                e.kind(),
                format!("cannot open the terminal: {e}"),
            ));
        }
    };

    let result = (|| -> std::io::Result<()> {
        let mut source = TerminalEvents;
        let mut last_frame = Instant::now();
        app.frame(terminal.get_frame().area());

        loop {
            if app.take_redraw() {
                terminal.draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;
            }

            let timeout = FRAME_RATE.saturating_sub(last_frame.elapsed());
            events::handle_events(&mut app, &mut source, timeout)?;

            if last_frame.elapsed() >= FRAME_RATE {
                last_frame = Instant::now();
                app.frame(terminal.get_frame().area());
            }

            if app.should_quit {
                break;
            }
        }
        Ok(())
    })();

    ratatui::restore();
    result
}
