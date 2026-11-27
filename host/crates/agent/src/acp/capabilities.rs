//! What an agent said it can do when it started (its `initialize` answer), and which adapter it is where adapters
//! differ.

use serde_json::Value;

/// An agent's capabilities, from its `initialize` answer.
// Each flag is one capability the agent advertised, independent of the others.
#[allow(clippy::struct_excessive_bools, reason = "independent facts the agent advertised")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Capabilities {
    /// `session/load`: reopens a session it isn't holding, replaying its history first.
    pub load: bool,
    /// `session/resume`: reopens one without the replay.
    pub resume: bool,
    /// `session/close`: an idle session can be released while the agent keeps running.
    pub close: bool,
    /// ACP `systemPrompt`: instructions on `session/new` and `load` that the agent keeps at system level.
    pub system_prompt: bool,
    /// ACP `additionalDirectories`: folders outside `cwd` the agent may work in.
    pub additional_directories: bool,
    /// `_session/steering` (not in ACP v1 yet, in Claude's, Codex's and omp's adapters' `_meta`): a message joins the
    /// running turn ([`Session::steer`](super::Session::steer)).
    pub steering: bool,
    /// `session/fork`: a new session that starts as a copy of another.
    pub fork: bool,
    /// `session/delete`: a session leaves the agent's saved history.
    pub delete: bool,
    /// It reports the work it runs in the background ([`background_tasks_meta`](super::claude::background_tasks_meta)).
    pub tasks: bool,
    pub adapter: Adapter,
    /// The agent's name and version (`agentInfo`).
    pub name: Option<String>,
    pub version: Option<String>,
}

impl Capabilities {
    pub fn from_initialize(init: &Value) -> Self {
        let caps = &init["agentCapabilities"];
        let session = |key: &str| caps["sessionCapabilities"][key].is_object();
        let info = |key: &str| init["agentInfo"][key].as_str().map(str::to_owned);
        Self {
            load: caps["loadSession"].as_bool().unwrap_or(false),
            resume: session("resume"),
            close: session("close"),
            system_prompt: session("systemPrompt"),
            additional_directories: session("additionalDirectories"),
            steering: init["_meta"]["steering"]["supported"] == true,
            fork: session("fork"),
            delete: session("delete"),
            tasks: super::claude::reports_tasks(init),
            adapter: Adapter::of(init),
            name: info("name"),
            version: info("version"),
        }
    }

    /// How a session it isn't holding opens again, when it can: `load` first.
    pub fn reopen(&self) -> Option<Reopen> {
        if self.load { Some(Reopen::Load) } else { self.resume.then_some(Reopen::Resume) }
    }

    /// An OpenCode older than 2 retries a model call its provider refused forever (a used-up free quota, a bad key), so
    /// such a turn never ends (anomalyco/opencode#41848); 2.x returns the error at once.
    pub fn never_ends_refused_turns(&self) -> bool {
        self.is_opencode()
            && self.version.as_deref().and_then(|v| semver::Version::parse(v).ok()).is_some_and(|v| v.major < 2)
    }

    /// OpenCode 2.x drops the app's MCP servers from its sessions once it has been idle a while (#508).
    pub fn drops_servers(&self) -> bool {
        self.is_opencode() && !self.never_ends_refused_turns()
    }

    fn is_opencode(&self) -> bool {
        self.name.as_deref() == Some("OpenCode")
    }
}

/// How a session reopens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reopen {
    Load,
    Resume,
}

impl Reopen {
    /// Its request.
    pub fn method(self) -> &'static str {
        match self {
            Self::Load => "session/load",
            Self::Resume => "session/resume",
        }
    }
}

/// Which adapter answered `initialize`, where an app treats it apart from the others.
// Not `non_exhaustive`: `Other` stands for every adapter not named.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Adapter {
    /// claude-agent-acp: takes `_meta.systemPrompt` and `_meta.claudeCode.options`, and `/compact <instructions>`.
    Claude,
    /// codex-acp: `/compact`.
    Codex,
    #[default]
    Other,
}

impl Adapter {
    fn of(init: &Value) -> Self {
        if init["agentCapabilities"]["_meta"]["claudeCode"].is_object() {
            Self::Claude
        } else if init["agentInfo"]["name"].as_str().is_some_and(|name| name.ends_with("codex-acp")) {
            Self::Codex
        } else {
            Self::Other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn opencode(version: &str) -> Capabilities {
        Capabilities::from_initialize(&json!({"agentInfo": {"name": "OpenCode", "version": version}}))
    }

    #[test]
    fn only_opencode_before_2_never_ends_a_refused_turn() {
        assert!(opencode("1.19.5").never_ends_refused_turns());
        assert!(!opencode("2.2.6").never_ends_refused_turns());
        assert!(!opencode("dev").never_ends_refused_turns());
        let kilo = Capabilities::from_initialize(&json!({"agentInfo": {"name": "Kilo", "version": "1.0.0"}}));
        assert!(!kilo.never_ends_refused_turns());
        assert!(!Capabilities::from_initialize(&json!({})).never_ends_refused_turns());
        assert!(opencode("2.2.6").drops_servers() && !opencode("1.19.5").drops_servers() && !kilo.drops_servers());
    }

    #[test]
    fn a_session_reopens_by_load_before_resume() {
        let both = json!({"agentCapabilities": {"loadSession": true, "sessionCapabilities": {"resume": {}}}});
        assert_eq!(Capabilities::from_initialize(&both).reopen(), Some(Reopen::Load));
        let resume = json!({"agentCapabilities": {"sessionCapabilities": {"resume": {}, "fork": {}}}});
        let caps = Capabilities::from_initialize(&resume);
        assert_eq!((caps.reopen(), caps.fork, caps.close), (Some(Reopen::Resume), true, false));
        assert_eq!(Capabilities::from_initialize(&json!({})).reopen(), None);
    }

    #[test]
    fn the_adapter_is_told_from_its_answer() {
        let claude =
            json!({"agentCapabilities": {"_meta": {"claudeCode": {}}}, "_meta": {"steering": {"supported": true}}});
        let caps = Capabilities::from_initialize(&claude);
        assert_eq!((caps.adapter, caps.steering), (Adapter::Claude, true));
        let codex = json!({"agentInfo": {"name": "@zed-industries/codex-acp"}});
        assert_eq!(Capabilities::from_initialize(&codex).adapter, Adapter::Codex);
        assert_eq!(Capabilities::from_initialize(&json!({})).adapter, Adapter::Other);
    }
}
