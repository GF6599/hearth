use std::time::{Duration, Instant};

use clap::Parser;
use hearth::app::App;
use hearth::cli::Cli;
use hearth::events::{self, TerminalEvents};

/// About 16 frames per second. Fast enough for rain and fire, slow enough
/// that an idle terminal stays cheap.
const TICK_RATE: Duration = Duration::from_millis(60);

fn main() {
    let cli = Cli::parse();
    let app = App::new(cli.scene, cli.seed());
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
