//! `codync-host tui`: the Codync client for a terminal. It talks to a host over the
//! same HTTP + SSE API as the phone and desktop apps (locally: no pairing needed).

mod app;
mod md;
mod net;
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

pub async fn run(url: String, token: Option<String>) -> Result<()> {
    let client = net::Client::new(&url, token.as_deref());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = app::App::new(client.clone(), tx.clone(), url);

    terminal::enable_raw_mode().context("this needs an interactive terminal")?;
    let mut out = std::io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste, EnableFocusChange)?;
    // Lets terminals that support it report shift+enter apart from enter.
    let enhanced = terminal::supports_keyboard_enhancement().unwrap_or(false)
        && execute!(out, PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)).is_ok();
    let _restore = Restore { enhanced };
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore(enhanced);
        hook(info);
    }));
    let mut term = Terminal::new(CrosstermBackend::new(out))?;

    app.call("hello", serde_json::json!({}), app::After::Hello);
    client.spawn_stream(tx);
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dirty = true;
    loop {
        if dirty {
            app.tick();
            term.draw(|f| view::draw(f, &mut app))?;
            dirty = false;
        }
        tokio::select! {
            ev = events.next() => match ev {
                Some(Ok(Event::Key(k))) if k.kind != KeyEventKind::Release => app.on_key(k),
                Some(Ok(Event::Paste(s))) => app.on_paste(&s),
                Some(Ok(Event::Mouse(m))) => app.on_mouse(m),
                Some(Ok(Event::FocusGained)) => app.term_focused = true,
                Some(Ok(Event::FocusLost)) => app.term_focused = false,
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
            _ = tick.tick() => {
                if !app.animating() {
                    continue;
                }
                app.frame += 1;
            }
        }
        dirty = true;
        if app.quit {
            break;
        }
    }
    Ok(())
}
