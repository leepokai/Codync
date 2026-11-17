//! A bot's dev servers: `/servers` or a click on the port in the chat's header. Clients name a server and never send a
//! command.

use crossterm::event::{KeyCode, KeyEvent};
use serde::Deserialize;
use serde_json::json;

use super::super::app::{After, App, Overlay, copy};
use super::super::l10n::tr;
use super::{Reply, open_browser};
use crate::analytics::reported::{DevServerAction, Reported};
use crate::dev_servers::Status;

const STARTED_BY_SHELL: &str = "Started through the bot's shell: it stops with the bot";
const ONLY_ON_COMPUTER: &str = "Only reachable on the computer";

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DevServer {
    pub id: String,
    pub name: String,
    pub command: String,
    pub status: Status,
    pub ports: Vec<DevServerPort>,
    pub exit_code: Option<i32>,
    pub managed: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct DevServerPort {
    pub port: u16,
    pub local: String,
    pub remote: Vec<String>,
}

impl DevServer {
    /// What its row shows beside the name.
    pub fn value(&self) -> String {
        match self.status {
            Status::Starting => "Starting".to_owned(),
            Status::Failed => "Failed to start".to_owned(),
            Status::Stopped => "Stopped".to_owned(),
            Status::Running if self.ports.is_empty() => "Running".to_owned(),
            Status::Running => self.ports.iter().map(|p| format!(":{}", p.port)).collect::<Vec<_>>().join(" "),
        }
    }

    fn opens(&self) -> bool {
        self.status == Status::Running && !self.ports.is_empty()
    }

    /// The port to use: on its computer the first, what a browser there opens; elsewhere the first that other devices
    /// can reach, or the first if none listens beyond its computer.
    fn port_from(&self, here: bool) -> Option<&DevServerPort> {
        let beyond = if here { None } else { self.ports.iter().find(|port| !port.remote.is_empty()) };
        beyond.or_else(|| self.ports.first())
    }

    /// Where it can be reached from elsewhere, a line each: every address, since this device may reach only some of them.
    pub fn reachable(&self) -> Vec<String> {
        let Some(port) = self.port_from(false).filter(|_| self.opens()) else { return Vec::new() };
        if port.remote.is_empty() {
            vec![format!("{} · {}", port.local, tr(ONLY_ON_COMPUTER))]
        } else {
            port.remote.clone()
        }
    }

    fn restarts(&self) -> bool {
        self.managed && self.status != Status::Starting
    }

    fn stops(&self) -> bool {
        self.status.active()
    }

    fn removes(&self) -> bool {
        self.managed && !self.status.active()
    }

    /// What the selected server's keys do: the sheet's footer offers only these.
    pub fn key_hints(&self) -> Vec<&'static str> {
        let mut keys = vec!["↑↓ select"];
        if self.opens() {
            keys.extend(["enter open", "c copy"]);
        } else if self.managed {
            keys.push("enter logs");
        }
        if self.managed {
            keys.push("l logs");
        }
        if self.restarts() {
            keys.push(if self.status.active() { "r restart" } else { "r start" });
        }
        if self.stops() {
            keys.push("s stop");
        }
        if self.removes() {
            keys.push("x remove");
        }
        keys.push("esc close");
        keys
    }

    /// What the sheet says of the selected server beyond its row.
    pub fn note(&self) -> String {
        if self.managed { self.exit_note().unwrap_or_default() } else { tr(STARTED_BY_SHELL).into_owned() }
    }

    fn exit_note(&self) -> Option<String> {
        let code = self.exit_code.filter(|_| self.status == Status::Failed)?;
        Some(tr("Exit code {code}").replace("{code}", &code.to_string()))
    }
}

/// What the chat's header shows of a bot's servers.
#[derive(Debug, PartialEq, Eq)]
pub enum DevServerSummary {
    Running(String),
    Starting,
    Failed,
}

pub fn summary(servers: &[DevServer]) -> Option<DevServerSummary> {
    let running: Vec<&DevServer> = servers.iter().filter(|s| s.status == Status::Running).collect();
    let first = running.iter().find(|s| !s.ports.is_empty()).or_else(|| running.first());
    if let Some(first) = first {
        let label = first.ports.first().map_or_else(|| first.name.clone(), |p| format!(":{}", p.port));
        let more = running.len() - 1;
        return Some(DevServerSummary::Running(if more > 0 { format!("{label} +{more}") } else { label }));
    }
    if servers.iter().any(|s| s.status == Status::Starting) {
        return Some(DevServerSummary::Starting);
    }
    servers.iter().any(|s| s.status == Status::Failed).then_some(DevServerSummary::Failed)
}

pub struct DevServersSheet {
    pub bot: String,
    pub selected: usize,
    /// The selected server's log, once opened: `None` lists the servers.
    pub logs: Option<LogsView>,
}

pub struct LogsView {
    pub id: String,
    pub name: String,
    /// The line under its name: the command, and how it ended if it failed.
    pub detail: String,
    pub text: Option<String>,
    /// Why the last read failed. What an earlier one read stays in `text`.
    pub error: Option<String>,
    pub scroll: usize,
}

impl App {
    pub(in crate::tui) fn open_dev_servers(&mut self) {
        let Some(bot) = self.bot().filter(|bot| !bot.dev_servers.is_empty()).map(|bot| bot.id.clone()) else { return };
        self.overlays.push(Overlay::DevServers(DevServersSheet { bot, selected: 0, logs: None }));
    }

    /// The answer to `read_logs`, for the sheet still showing that server.
    pub(super) fn dev_server_logs(&mut self, id: &str, read: Result<String, String>) {
        for overlay in &mut self.overlays {
            if let Overlay::DevServers(sheet) = overlay
                && let Some(logs) = sheet.logs.as_mut().filter(|logs| logs.id == id)
            {
                match read {
                    Ok(text) => {
                        logs.text = Some(text);
                        logs.error = None;
                    }
                    Err(error) => logs.error = Some(error),
                }
                return;
            }
        }
    }

    fn servers_of(&self, bot: &str) -> Vec<DevServer> {
        self.bots.get(bot).map(|bot| bot.dev_servers.clone()).unwrap_or_default()
    }

    pub(in crate::tui) fn dev_servers_key(&mut self, mut sheet: DevServersSheet, k: KeyEvent) -> Option<Overlay> {
        if let Some(logs) = &mut sheet.logs {
            match k.code {
                KeyCode::Esc | KeyCode::Char('q') => sheet.logs = None,
                KeyCode::Up | KeyCode::Char('k') => logs.scroll += 1,
                KeyCode::Down | KeyCode::Char('j') => logs.scroll = logs.scroll.saturating_sub(1),
                KeyCode::Char('r') => self.read_logs(&sheet.bot, &logs.id),
                _ => {}
            }
            return Some(Overlay::DevServers(sheet));
        }
        let servers = self.servers_of(&sheet.bot);
        if servers.is_empty() {
            return None;
        }
        sheet.selected = sheet.selected.min(servers.len() - 1);
        let server = &servers[sheet.selected];
        let call = |app: &Self, method: &'static str| {
            app.call(method, json!({"botId": sheet.bot, "id": server.id}), After::Nothing);
        };
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Up | KeyCode::Char('k') => sheet.selected = (sheet.selected + servers.len() - 1) % servers.len(),
            KeyCode::Down | KeyCode::Char('j') => sheet.selected = (sheet.selected + 1) % servers.len(),
            KeyCode::Enter if server.opens() => self.open_dev_server(server),
            KeyCode::Enter | KeyCode::Char('l') if server.managed => {
                sheet.logs = Some(self.logs_of(&sheet.bot, server));
            }
            KeyCode::Char('c') if server.opens() => self.copy_dev_server(server),
            KeyCode::Char('r') if server.restarts() => call(self, "devServerRestart"),
            KeyCode::Char('s') if server.stops() => call(self, "devServerStop"),
            KeyCode::Char('x') if server.removes() => call(self, "devServerRemove"),
            _ => {}
        }
        Some(Overlay::DevServers(sheet))
    }

    fn read_logs(&self, bot: &str, id: &str) {
        self.call("devServerLogs", json!({"botId": bot, "id": id}), After::Sheet(Reply::DevServerLogs(id.to_owned())));
    }

    fn logs_of(&self, bot: &str, server: &DevServer) -> LogsView {
        self.read_logs(bot, &server.id);
        let detail = match server.exit_note() {
            Some(note) => format!("{} · {note}", server.command),
            None => server.command.clone(),
        };
        LogsView { id: server.id.clone(), name: server.name.clone(), detail, text: None, error: None, scroll: 0 }
    }

    /// In this computer's browser. Anywhere else (a tunnel looks the same from here) shows where it can be reached.
    fn open_dev_server(&mut self, server: &DevServer) {
        if self.on_this_computer && server.port_from(true).is_some_and(|port| open_browser(&port.local, &self.url)) {
            self.track_dev_server(DevServerAction::Open);
            return;
        }
        self.hint =
            Some((server.reachable().join(" · "), std::time::Instant::now() + std::time::Duration::from_secs(5)));
    }

    fn copy_dev_server(&mut self, server: &DevServer) {
        let here = self.on_this_computer;
        let Some(port) = server.port_from(here) else { return };
        copy(if here { &port.local } else { port.remote.first().unwrap_or(&port.local) });
        self.track_dev_server(DevServerAction::Copy);
        self.flash(if here || !port.remote.is_empty() { "Copied" } else { ONLY_ON_COMPUTER });
    }

    fn track_dev_server(&self, action: DevServerAction) {
        self.track(Reported::DevServerAction { action });
    }
}

#[cfg(test)]
mod tests {
    use super::{DevServer, DevServerPort, DevServerSummary, Status, summary, tr};

    fn server(status: Status, port: Option<u16>) -> DevServer {
        DevServer {
            name: "watch".into(),
            status,
            ports: port.map(|port| DevServerPort { port, ..Default::default() }).into_iter().collect(),
            managed: true,
            ..Default::default()
        }
    }

    #[test]
    fn the_header_shows_a_port_and_how_many_more_run() {
        assert_eq!(summary(&[]), None);
        assert_eq!(summary(&[server(Status::Stopped, None)]), None);
        assert_eq!(summary(&[server(Status::Running, Some(5173))]), Some(DevServerSummary::Running(":5173".into())));
        let many =
            [server(Status::Running, None), server(Status::Running, Some(5173)), server(Status::Running, Some(8787))];
        assert_eq!(summary(&many), Some(DevServerSummary::Running(":5173 +2".into())));
        assert_eq!(summary(&[server(Status::Running, None)]), Some(DevServerSummary::Running("watch".into())));
    }

    #[test]
    fn running_outranks_starting_which_outranks_failed() {
        let failed = server(Status::Failed, None);
        assert_eq!(summary(&[failed.clone(), server(Status::Starting, None)]), Some(DevServerSummary::Starting));
        assert_eq!(summary(std::slice::from_ref(&failed)), Some(DevServerSummary::Failed));
        assert_eq!(summary(&[failed, server(Status::Running, Some(1))]), Some(DevServerSummary::Running(":1".into())));
    }

    #[test]
    fn elsewhere_it_shows_every_address_or_says_it_stays_on_the_computer() {
        let port = |remote: &[&str]| DevServerPort {
            port: 5173,
            local: "http://localhost:5173".into(),
            remote: remote.iter().map(ToString::to_string).collect(),
        };
        let with = |ports| DevServer { ports, ..server(Status::Running, None) };
        let (tailscale, lan) = ("http://100.64.0.1:5173", "http://192.168.1.20:5173");
        assert_eq!(with(vec![port(&[lan])]).reachable(), [lan]);
        assert_eq!(with(vec![port(&[tailscale, lan])]).reachable(), [tailscale, lan]);
        let only_here = with(vec![port(&[])]).reachable();
        assert!(only_here[0].starts_with("http://localhost:5173 · "), "{only_here:?}");
        // The first port listens only on the computer; the second doesn't.
        assert_eq!(with(vec![port(&[]), port(&[lan])]).reachable(), [lan]);
        assert_eq!(with(vec![]).reachable(), Vec::<String>::new());
        let stopped = DevServer { ports: vec![port(&[lan])], ..server(Status::Stopped, None) };
        assert_eq!(stopped.reachable(), Vec::<String>::new());
    }

    #[test]
    fn on_its_computer_it_uses_the_first_port_and_elsewhere_the_first_others_can_reach() {
        let port = |port, remote: &[&str]| DevServerPort {
            port,
            remote: remote.iter().map(ToString::to_string).collect(),
            ..Default::default()
        };
        let mixed = DevServer {
            ports: vec![port(3000, &[]), port(5173, &["http://192.168.1.20:5173"])],
            ..server(Status::Running, None)
        };
        assert_eq!(mixed.port_from(true).map(|p| p.port), Some(3000));
        assert_eq!(mixed.port_from(false).map(|p| p.port), Some(5173));
        let only_here = DevServer { ports: vec![port(3000, &[])], ..server(Status::Running, None) };
        assert_eq!(only_here.port_from(false).map(|p| p.port), Some(3000));
    }

    #[test]
    fn the_footer_offers_only_the_keys_that_apply() {
        let keys = |server: &DevServer| server.key_hints().join(" · ");
        assert_eq!(
            keys(&server(Status::Running, Some(1))),
            "↑↓ select · enter open · c copy · l logs · r restart · s stop · esc close"
        );
        assert_eq!(
            keys(&server(Status::Running, None)),
            "↑↓ select · enter logs · l logs · r restart · s stop · esc close"
        );
        assert_eq!(keys(&server(Status::Starting, None)), "↑↓ select · enter logs · l logs · s stop · esc close");
        assert_eq!(
            keys(&server(Status::Failed, None)),
            "↑↓ select · enter logs · l logs · r start · x remove · esc close"
        );
        let shell = DevServer { managed: false, ..server(Status::Running, Some(1)) };
        assert_eq!(keys(&shell), "↑↓ select · enter open · c copy · s stop · esc close");
    }

    #[test]
    fn a_status_from_a_newer_host_reads_as_stopped() {
        let server: DevServer = serde_json::from_str(r#"{"status": "sleeping", "managed": true}"#).expect("it decodes");
        assert_eq!(server.status, Status::Stopped);
        assert_eq!(server.value(), "Stopped");
    }

    #[test]
    fn the_sheet_says_how_it_ended_or_that_it_ends_with_the_bot() {
        let failed = DevServer { exit_code: Some(3), ..server(Status::Failed, None) };
        // Through `tr`: another test may have set the process's language.
        assert_eq!(failed.note(), tr("Exit code {code}").replace("{code}", "3"));
        assert_eq!(server(Status::Running, None).note(), "");
        let shell = DevServer { managed: false, ..server(Status::Running, None) };
        assert_eq!(shell.note(), tr("Started through the bot's shell: it stops with the bot"));
    }
}
