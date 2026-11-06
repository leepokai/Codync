//! `codync-host tui`: the Codync client for a terminal. It talks to a host over the
//! same HTTP + SSE API as the phone and desktop apps (locally: no pairing needed).

mod app;
mod appearance;
mod choice;
mod connections;
mod demo;
mod details;
mod drafts;
mod l10n;
mod manage;
mod md;
pub mod net;
mod sheets;
mod view;

use anyhow::{Context, Result};
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste, EnableFocusChange,
    EnableMouseCapture, Event, EventStream, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::Write as _;
use std::time::Duration;

/// Puts the terminal back however we leave (including a panic).
struct Restore {
    enhanced: bool,
}

impl Drop for Restore {
    fn drop(&mut self) {
        restore(self.enhanced);
    }
}

fn restore(enhanced: bool) {
    let mut out = std::io::stdout();
    if enhanced {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(out, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();
}

/// The app the person is looking at: the demo's while it is on, else the real one (which keeps running underneath).
macro_rules! active {
    ($app:ident, $demo:ident) => {
        match $demo.as_mut() {
            Some((demo, _)) => demo,
            None => &mut $app,
        }
    };
}

pub async fn run(url: String, token: Option<String>) -> Result<()> {
    let client = net::Client::new(&url, token.as_deref());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = app::App::new(client.clone(), tx.clone(), url);
    let mut drafts = match drafts::Drafts::load(&app.url).await {
        Ok(saved) => {
            app.drafts = saved.restore();
            Some(saved)
        }
        Err(error) => {
            app.flash(&format!("Couldn't restore drafts: {error}"));
            None
        }
    };

    terminal::enable_raw_mode().context("this needs an interactive terminal")?;
    let mut out = std::io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste, EnableFocusChange)?;
    // Lets terminals that support it report shift+enter apart from enter.
    let enhanced = terminal::supports_keyboard_enhancement().unwrap_or(false)
        && execute!(out, PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)).is_ok();
    let _restore = Restore { enhanced };
    let hook = std::panic::take_hook();
    let reporter = client.clone();
    std::panic::set_hook(Box::new(move |info| {
        restore(enhanced);
        reporter.report_panic(info);
        hook(info);
    }));
    let mut term = Terminal::new(CrosstermBackend::new(out))?;

    app.call("hello", serde_json::json!({}), app::After::Hello);
    client.spawn_stream(tx);
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dirty = true;
    // Demo mode: a second app on a script, with its own channel; leaving drops it and the real one is as it was.
    let mut demo: Option<(app::App, tokio::sync::mpsc::UnboundedReceiver<app::Msg>)> = None;
    // A setup terminal has the real screen (not the alternate one) while it runs.
    let mut in_shell = false;
    loop {
        if app.shell.is_some() != in_shell {
            in_shell = !in_shell;
            let out = term.backend_mut();
            if in_shell {
                execute!(out, DisableMouseCapture, LeaveAlternateScreen)?;
                write!(out, "\x1b[2J\x1b[H\x1b[2m^] closes the terminal and goes back to Codync\x1b[0m\r\n")?;
            } else {
                execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
                term.clear()?;
            }
        }
        if let Some(sh) = app.shell.as_mut().filter(|s| !s.out.is_empty()) {
            let out = term.backend_mut();
            out.write_all(&std::mem::take(&mut sh.out))?;
            out.flush()?;
        }
        if dirty && !in_shell {
            let shown = active!(app, demo);
            shown.tick();
            shown.report_screen();
            term.draw(|f| view::draw(f, shown))?;
            dirty = false;
        }
        tokio::select! {
            ev = events.next() => match ev {
                Some(Ok(Event::Key(k))) if k.kind != KeyEventKind::Release => active!(app, demo).on_key(k),
                Some(Ok(Event::Paste(s))) => active!(app, demo).on_paste(&s),
                Some(Ok(Event::Mouse(m))) => active!(app, demo).on_mouse(m),
                Some(Ok(Event::FocusGained)) => active!(app, demo).term_focused = true,
                Some(Ok(Event::FocusLost)) => active!(app, demo).term_focused = false,
                Some(Ok(Event::Resize(cols, rows))) => {
                    app.on_resize(cols, rows);
                    if let Some((d, _)) = demo.as_mut() {
                        d.on_resize(cols, rows);
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break,
            },
            Some(msg) = rx.recv() => {
                app.on_msg(msg);
                // Apply a burst (catch-up) before drawing once.
                while let Ok(msg) = rx.try_recv() {
                    app.on_msg(msg);
                }
            }
            Some(msg) = async {
                match demo.as_mut() {
                    Some((_, demo_rx)) => demo_rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some((d, demo_rx)) = demo.as_mut() {
                    d.on_msg(msg);
                    while let Ok(msg) = demo_rx.try_recv() {
                        d.on_msg(msg);
                    }
                }
            }
            _ = tick.tick() => {
                let shown = active!(app, demo);
                if !shown.animating() {
                    continue;
                }
                shown.frame += 1;
            }
        }
        dirty = true;
        if app.demo_toggle {
            app.demo_toggle = false;
            if demo.is_none() {
                let (demo_tx, demo_rx) = tokio::sync::mpsc::unbounded_channel();
                let client = net::Client::demo();
                let mut shown = app::App::new(client.clone(), demo_tx.clone(), "demo".to_owned());
                shown.demo = true;
                shown.call("hello", serde_json::json!({}), app::After::Hello);
                client.spawn_stream(demo_tx);
                demo = Some((shown, demo_rx));
            }
        }
        if demo.as_ref().is_some_and(|(d, _)| d.quit) {
            demo = None;
        }
        if let Some(saved) = drafts.as_mut()
            && let Err(error) = saved.save(&app.drafts).await
        {
            app.flash(&format!("Couldn't save drafts: {error}. Text stays in this session."));
        }
        if app.quit {
            break;
        }
    }
    Ok(())
}
