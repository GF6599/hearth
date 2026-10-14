use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use hearth::app::App;
use hearth::cli::Cli;
use hearth::config::{self, Config};
use hearth::events::{self, TerminalEvents};
use hearth::scene::TICK_RATE;
use hearth::scenes::typing;

fn main() {
    let cli = Cli::parse();
    let config = Config::load(&config::default_path()).unwrap_or_else(|e| {
        eprintln!("hearth: {e}");
        std::process::exit(1);
    });
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
    let app = cli.app(&config, corpus);
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
        let mut last_tick = Instant::now();
        app.tick(terminal.get_frame().area());

        loop {
            terminal.draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;

            let timeout = TICK_RATE.saturating_sub(last_tick.elapsed());
            events::handle_events(&mut app, &mut source, timeout)?;

            if last_tick.elapsed() >= TICK_RATE {
                last_tick = Instant::now();
                app.tick(terminal.get_frame().area());
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
