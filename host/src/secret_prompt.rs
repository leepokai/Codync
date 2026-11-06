//! Reads a secret for a CLI command: typed without echo on a terminal, or one line of piped stdin.

use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal;
use std::io::{BufRead, IsTerminal, Write};

/// One line of secret input, trimmed. Typed keys are never shown.
pub fn read(prompt: &str) -> Result<String> {
    if !std::io::stdin().is_terminal() {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        return Ok(line.trim().to_owned());
    }
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    terminal::enable_raw_mode()?;
    let typed = read_keys();
    terminal::disable_raw_mode()?;
    eprintln!();
    typed
}

fn read_keys() -> Result<String> {
    let mut secret = String::new();
    loop {
        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Enter => return Ok(secret.trim().to_owned()),
            KeyCode::Backspace => {
                secret.pop();
            }
            KeyCode::Esc => bail!("cancelled"),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => bail!("cancelled"),
            KeyCode::Char(c) => secret.push(c),
            _ => {}
        }
    }
}
