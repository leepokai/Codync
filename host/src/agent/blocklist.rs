//! Commands no bot may run, in any profile and with any approval setting: wiping the root or the home folder,
//! formatting a disk, raising to root, shutting the computer down. They are checked before the user's rules and
//! before a bot set to approve everything, so even `Full` refuses them.
//!
//! A pattern is a glob over a whole command (`approval_rules::glob`): `*` is any text and `?` one character. In a
//! pattern, `[*]` is a literal star: `rm -rf /[*]` is the contents of the root, not "anything under /". Mercury's
//! list writes `/*` for both, which would refuse `rm -rf /tmp/build`; here only the root itself and its contents are
//! refused. The command is split at `;`, `&`, `|`, newlines, brackets and backticks, and each part is checked after
//! its shell wrappers (`sudo`-less `env`, `cmd /c`, `powershell -Command`, `bash -c`) are opened.
//!
//! List ported from mercury-agent's `src/capabilities/shell/blocklist.ts` (MIT, Copyright (c) 2026 Cosmic Stack;
//! `THIRD-PARTY-NOTICES.md`), with the Windows variants it generates; the entries marked "added" are not in that
//! file. The wrapper and chain handling is Codync's own. See `docs/features/approvals.md`.

use super::approval_rules::glob;
use std::sync::LazyLock;

/// What stands for a literal star after [`normalize`].
const STAR: char = '\u{1}';
/// How deep a wrapper may hide a command (`bash -c "powershell -c '…'"`).
const WRAPPER_DEPTH: usize = 3;
/// Characters that end one command and start another.
const SEPARATORS: [char; 8] = [';', '&', '|', '\n', '(', ')', '`', '{'];

/// The fixed list, `[*]` marking a literal star.
const COMMANDS: &[&str] = &[
    "sudo *",
    "rm -rf /",
    "rm -rf ~",
    "rm -rf /[*]",
    "rm -fr /",
    "rm -fr ~",
    "rm -fr /[*]",
    "rm -rf .",
    "rm -rf ..",
    "mkfs *",
    "dd if=*",
    "chmod 777 /",
    "chown * /",
    ":(){ :|:& };:",
    "shutdown *",
    "reboot *",
    "halt *",
    "init 0",
    "init 6",
    "kill -9 1",
    "> /dev/sda",
    "mv /[*] /dev/null",
    // Windows, cmd.exe.
    "format *",
    "icacls * ?:\\[*] /grant",
    "net user *",
    "netsh *",
    "reg delete *",
    // PowerShell: execution-policy changes disable the script-signing guard for the machine or the user.
    "Set-ExecutionPolicy *",
    // Added: the same wipes with the flags in the other order, or with a no-preserve flag.
    "rm -rf / *",
    "rm -fr / *",
    "rm -rf ~/",
    "rm -rf ~/[*]",
    "rm -fr ~/",
    "rm -fr ~/[*]",
    "rm -r -f /",
    "rm -f -r /",
    "rm --no-preserve-root *",
    "rm -rf --no-preserve-root *",
];

/// `rd`, `rmdir`, `del` and `erase` aimed at a drive root (`C:\`) or its contents (`C:\*`), with any flags.
const CMD_DELETES: [&str; 4] = ["rd", "rmdir", "del", "erase"];

/// PowerShell's `Remove-Item` and its aliases.
const POWERSHELL_DELETES: [&str; 7] = ["Remove-Item", "rm", "ri", "rmdir", "rd", "del", "erase"];

/// Where a recursive delete is aimed: a drive root, the home folder or the root, alone or with its contents.
const ROOT_TARGETS: [&str; 7] = ["?:\\", "?:\\[*]", "~", "~\\[*]", "~/[*]", "/", "/[*]"];

static PATTERNS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut all: Vec<String> = COMMANDS.iter().map(|p| (*p).to_owned()).collect();
    for cmd in CMD_DELETES {
        all.push(format!("{cmd} *?:\\"));
        all.push(format!("{cmd} *?:\\[*]"));
    }
    for cmd in POWERSHELL_DELETES {
        for target in ROOT_TARGETS {
            all.push(format!("{cmd} -r* {target}")); // Remove-Item -Recurse -Force C:\
            all.push(format!("{cmd} -r* * {target}")); // Remove-Item -Recurse -Force -Path C:\
            all.push(format!("{cmd} {target} -r*")); // Remove-Item C:\ -Recurse
            all.push(format!("{cmd} {target} * -r*")); // Remove-Item C:\ -Force -Recurse
            all.push(format!("{cmd} -Path {target} -r*"));
            all.push(format!("{cmd} -Path {target} * -r*"));
        }
    }
    all.iter().map(|p| p.replace("[*]", &STAR.to_string())).collect()
});

/// The pattern that forbids `command` (any command of a chain, inside any wrapper), or `None`.
pub fn blocked(command: &str) -> Option<String> {
    check(command, WRAPPER_DEPTH)
}

fn check(command: &str, depth: usize) -> Option<String> {
    let whole = normalize(command);
    if let Some(hit) = first_match(&whole) {
        return Some(hit);
    }
    for part in command.split(SEPARATORS) {
        let tokens = tokens(part);
        if let Some(hit) = first_match(&tokens.join(" ")) {
            return Some(hit);
        }
        if let Some(inner) = opened(&tokens)
            && depth > 0
            && let Some(hit) = check(&inner, depth - 1)
        {
            return Some(hit);
        }
    }
    None
}

fn first_match(text: &str) -> Option<String> {
    PATTERNS.iter().find(|pattern| glob(pattern, text, true)).map(|p| p.replace(STAR, "*"))
}

/// Whitespace collapsed, quotes dropped and a literal star marked.
fn normalize(text: &str) -> String {
    let unquoted: String = text.chars().map(|c| if matches!(c, '"' | '\'') { ' ' } else { c }).collect();
    tokens(&unquoted).join(" ")
}

/// The words of a command without its quotes, with a literal star marked, after leading `NAME=value` assignments,
/// `env`, `nohup`, `time`, `exec` and `command`.
fn tokens(part: &str) -> Vec<String> {
    let cleaned = normalize_words(part);
    let mut words: &[String] = &cleaned;
    while let Some(first) = words.first() {
        let lower = first.to_lowercase();
        let assignment = first.contains('=') && !first.starts_with('-') && !first.contains('/');
        if assignment || matches!(lower.as_str(), "env" | "nohup" | "time" | "exec" | "command") {
            words = &words[1..];
        } else {
            break;
        }
    }
    words.to_vec()
}

fn normalize_words(part: &str) -> Vec<String> {
    part.chars()
        .map(|c| match c {
            '"' | '\'' => ' ',
            '*' => STAR,
            other => other,
        })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// The command a shell wrapper runs: what follows `-c` / `-Command` (PowerShell, bash, sh) or `/c` (cmd).
fn opened(tokens: &[String]) -> Option<String> {
    let program = tokens.first()?.to_lowercase();
    let program = program.rsplit(['\\', '/']).next().unwrap_or_default().trim_end_matches(".exe").to_owned();
    let switches: &[&str] = match program.as_str() {
        "powershell" | "pwsh" => &["-command", "-c"],
        "cmd" => &["/c", "/k"],
        "bash" | "sh" | "zsh" | "dash" => &["-c", "-lc"],
        _ => return None,
    };
    let at = tokens.iter().position(|t| switches.contains(&t.to_lowercase().as_str()))?;
    let rest = tokens[at + 1..].join(" ").replace(STAR, "*");
    (!rest.is_empty()).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(command: &str) -> bool {
        blocked(command).is_some()
    }

    #[test]
    fn every_entry_of_the_list_refuses_itself() {
        for pattern in COMMANDS {
            // A literal star stays a star; a wildcard becomes some text, a `?` one character.
            let command = pattern.replace("[*]", "\u{2}").replace('*', "x").replace('?', "C").replace('\u{2}', "*");
            assert!(refused(&command), "`{command}` (from `{pattern}`) is allowed");
        }
    }

    #[test]
    fn the_unix_wipes_are_refused_but_ordinary_deletes_are_not() {
        for command in
            ["rm -rf /", "rm -rf /*", "rm -rf ~", "rm -rf ~/", "rm -fr /", "rm -rf / --no-preserve-root", "sudo ls"]
        {
            assert!(refused(command), "{command}");
        }
        for command in ["rm -rf /tmp/build", "rm -rf ./target", "rm -rf node_modules", "ls /", "git status"] {
            assert!(!refused(command), "{command} should be allowed");
        }
    }

    #[test]
    fn the_windows_wipes_are_refused() {
        for command in [
            r"Remove-Item -Recurse -Force C:\",
            r"Remove-Item -Recurse -Force -Path C:\",
            r"Remove-Item C:\ -Recurse",
            r"Remove-Item C:\ -Force -Recurse",
            r"Remove-Item -Path C:\ -Recurse",
            r"Remove-Item -Recurse -Force C:\*",
            r"Remove-Item -Recurse -Force ~",
            r"rm -rf C:\",
            r"rd /s /q C:\",
            r"rd /s /q C:\*",
            r"rmdir /s /q D:\",
            r"del /f /s /q C:\*",
            r"del /s /q C:\*",
            r"format c:",
            r"Set-ExecutionPolicy Bypass",
            r"net user admin pass /add",
        ] {
            assert!(refused(command), "{command}");
        }
        for command in [
            r"Remove-Item -Recurse -Force C:\Users\me\proj\node_modules",
            r"rd /s /q C:\Users\me\proj\build",
            r"del /s /q build\*",
            r"Get-ChildItem C:\",
        ] {
            assert!(!refused(command), "{command} should be allowed");
        }
    }

    #[test]
    fn chains_wrappers_and_quotes_do_not_hide_a_command() {
        for command in [
            "echo ok && rm -rf /",
            "ls; rm -rf ~",
            "cat x | sudo tee y",
            "FOO=1 rm -rf /",
            "env rm -rf /",
            r#"bash -c "rm -rf /""#,
            r"sh -c 'echo hi; rm -rf /'",
            r"cmd /c rd /s /q C:\",
            r#"powershell -NoProfile -Command "Remove-Item -Recurse -Force C:\""#,
            r#"pwsh -c "Set-ExecutionPolicy Unrestricted""#,
            r#"bash -c "powershell -c 'Remove-Item -Recurse -Force C:\'""#,
            ":(){ :|:& };:",
        ] {
            assert!(refused(command), "{command}");
        }
        assert!(!refused(r#"git commit -m "never run rm -rf /""#), "words inside a message are not a command");
    }

    #[test]
    fn the_pattern_that_matched_is_named() {
        assert_eq!(blocked("shutdown now").as_deref(), Some("shutdown *"));
        assert_eq!(blocked("rm -rf /*").as_deref(), Some("rm -rf /*"));
        assert_eq!(blocked("ls"), None);
    }
}
