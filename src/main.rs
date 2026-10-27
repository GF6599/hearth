use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use hearth::app::App;
use hearth::cli::Cli;
use hearth::config::{self, Config};
use hearth::events::{self, TerminalEvents};
use hearth::scene::FRAME_RATE;
use hearth::scenes::typing;
use hearth::screen;
use hearth::sound::Speaker;

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
    let mut terminal = match screen::open() {
        Ok(terminal) => terminal,
        Err(e) => {
            ratatui::restore();
            return Err(std::io::Error::new(
                e.kind(),
                format!("cannot open the terminal: {e}"),
            ));
        }
    };

    let mut speaker = Speaker::open(app.seed(), app.sound_on())
        .map_err(|reason| app.without_sound(&reason))
        .ok();

    let result = (|| -> std::io::Result<()> {
        let mut source = TerminalEvents;
        let mut last_frame = Instant::now();
        app.frame(terminal.get_frame().area());

        loop {
            if app.take_redraw() {
                screen::draw(&mut terminal, &app)?;
            }

            let timeout = FRAME_RATE.saturating_sub(last_frame.elapsed());
            events::handle_events(&mut app, &mut source, timeout)?;

            if last_frame.elapsed() >= FRAME_RATE {
                last_frame = Instant::now();
                app.frame(terminal.get_frame().area());
                play(&mut app, &mut speaker);
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

/// Hands the focused pane's cues to the speaker. A speaker that fails is
/// closed, and the notice says why the sound stopped.
fn play(app: &mut App, speaker: &mut Option<Speaker>) {
    let cues = app.take_cues();
    let Some(open) = speaker else {
        return;
    };
    let failed = open
        .set_on(app.sound_on())
        .err()
        .or_else(|| open.take_error());
    match failed {
        Some(reason) => {
            app.without_sound(&reason);
            *speaker = None;
        }
        None => open.play(cues),
    }
}
