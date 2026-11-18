//! How each agent keeps a login in a folder of its own. The agents differ in which variables point at the folder,
//! where the login is stored, how it is signed in, and which files belong to the account and not to the user's
//! shared setup.

use crate::slot::{self, Shared};
use crate::{ACCOUNT_VAR, Error, identity};
use std::path::Path;

/// How an account of the agent is signed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignIn {
    /// Its CLI's own sign-in command, in a terminal.
    Terminal,
    /// The agent signs itself in when asked over ACP (`authenticate`), usually in a browser on the computer.
    Agent,
}

/// How to tell whether a slot is signed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignedIn {
    /// The same check as the computer's own login: the agent's CLI, run with the slot's environment.
    AgentCheck,
    /// Its CLI run with `args` lists the logins: signed in unless the output has `none`.
    Lists { args: &'static str, none: &'static str },
    /// The login is this file in the slot.
    File(&'static str),
    /// This JSON file in the slot holds the logins, by provider: signed in once it has one.
    Logins(&'static str),
}

pub struct Agent {
    pub id: &'static str,
    /// The variables that point at the slot: each at a folder in it (`""` is the slot itself).
    folders: &'static [(&'static str, &'static str)],
    /// Variables an account's process starts with as they are: overrides that would bypass its login, cleared.
    fixed: &'static [(&'static str, &'static str)],
    /// Where the computer's own login and setup live, under the user's home.
    home: &'static str,
    /// The folder in the slot that stands for `home` (`""` is the slot itself).
    mirror: &'static str,
    shared: Shared,
    /// Saved keys that would choose another login: a process on an account starts without them.
    pub login_keys: &'static [&'static str],
    /// The CLI arguments that sign a slot out, where the login isn't only files in it.
    pub sign_out: Option<&'static str>,
    pub sign_in: SignIn,
    pub signed_in: SignedIn,
    /// Where its files say who a login is; nowhere for one that keeps only a key or a refresh token.
    names: Option<identity::Names>,
}

const AGENTS: [Agent; 7] = [
    Agent {
        id: "claude",
        folders: &[("CLAUDE_CONFIG_DIR", "")],
        fixed: &[],
        home: ".claude",
        mirror: "",
        shared: Shared::Claude,
        login_keys: &slot::CLAUDE_LOGIN_ENV,
        // On macOS the login is in the Keychain, under a name made from the slot's path.
        sign_out: Some("auth logout"),
        sign_in: SignIn::Terminal,
        signed_in: SignedIn::AgentCheck,
        names: Some(identity::CLAUDE),
    },
    Agent {
        id: "codex",
        folders: &[("CODEX_HOME", "")],
        fixed: &[],
        home: ".codex",
        mirror: "",
        shared: Shared::AllBut {
            private: &["auth.json", "auth.json.lock", "auth.lock", "models_cache.json"],
            sessions: "sessions",
        },
        login_keys: &["OPENAI_API_KEY", "CODEX_API_KEY"],
        // Its keyring mode keeps the login outside the slot.
        sign_out: Some("logout"),
        sign_in: SignIn::Terminal,
        signed_in: SignedIn::AgentCheck,
        names: Some(identity::CODEX),
    },
    // OpenCode keeps its logins (and sessions) in its data folder; its setup is in `~/.config/opencode`, which
    // every account reads as it is. The variables are XDG's, so what the agent runs sees them too.
    Agent {
        id: "opencode",
        folders: &[("XDG_DATA_HOME", "data"), ("XDG_STATE_HOME", "state")],
        // A login handed over in the environment would win over the slot's.
        fixed: &[("OPENCODE_AUTH_CONTENT", "")],
        home: "",
        mirror: "",
        shared: Shared::Nothing,
        login_keys: &[],
        sign_out: None,
        sign_in: SignIn::Terminal,
        signed_in: SignedIn::Lists { args: "auth list", none: "0 credentials" },
        names: Some(identity::OPENCODE),
    },
    Agent {
        id: "devin",
        folders: &[("XDG_DATA_HOME", "data"), ("XDG_STATE_HOME", "state")],
        fixed: &[],
        home: "",
        mirror: "",
        shared: Shared::Nothing,
        login_keys: &[],
        sign_out: None,
        sign_in: SignIn::Terminal,
        signed_in: SignedIn::File("data/devin/credentials.toml"),
        names: None,
    },
    // Its ACP server keeps its own sign-in under `antigravity-acp` in the home, in the Keychain where there is
    // one: the second variable keeps it in the slot.
    Agent {
        id: "antigravity",
        folders: &[("GEMINI_HOME", "")],
        fixed: &[("AGY_ACP_FORCE_FILE_STORAGE", "1")],
        home: ".gemini",
        mirror: "",
        shared: Shared::Antigravity,
        login_keys: &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        sign_out: None,
        sign_in: SignIn::Agent,
        signed_in: SignedIn::File("antigravity-acp/acp_token.json"),
        names: None,
    },
    // Pi keeps its logins, sessions and setup in one folder.
    Agent {
        id: "pi",
        folders: &[("PI_CODING_AGENT_DIR", "")],
        fixed: &[],
        home: ".pi/agent",
        mirror: "",
        shared: Shared::AllBut { private: &["auth.json"], sessions: "sessions" },
        login_keys: &[],
        sign_out: None,
        sign_in: SignIn::Terminal,
        signed_in: SignedIn::Logins("auth.json"),
        names: Some(identity::PI),
    },
    // Gemini CLI takes another home folder and keeps everything in `.gemini` under it. With its encrypted storage
    // on, the login moves to one Keychain entry every home shares, so that stays off. Read from its source
    // (0.66.0), never run here.
    Agent {
        id: "gemini",
        folders: &[("GEMINI_CLI_HOME", "")],
        fixed: &[("GEMINI_FORCE_ENCRYPTED_FILE_STORAGE", "false")],
        home: ".gemini",
        mirror: ".gemini",
        shared: Shared::AllBut {
            private: &["oauth_creds.json", "google_accounts.json", "antigravity-acp"],
            sessions: "tmp",
        },
        login_keys: &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        sign_out: None,
        sign_in: SignIn::Terminal,
        signed_in: SignedIn::File(".gemini/oauth_creds.json"),
        names: Some(identity::GEMINI),
    },
];

/// Every agent that can have accounts.
pub fn agents() -> &'static [Agent] {
    &AGENTS
}

/// How `id`'s agent keeps accounts, if it can have them.
pub fn agent(id: &str) -> Option<&'static Agent> {
    AGENTS.iter().find(|a| a.id == id)
}

impl Agent {
    /// What a process needs to run on the account in `slot`.
    pub fn env(&self, slot: &Path, account: &str) -> Vec<(String, String)> {
        let folders = self.folders.iter().map(|(var, folder)| {
            let path = if folder.is_empty() { slot.to_path_buf() } else { slot.join(folder) };
            ((*var).to_owned(), path.to_string_lossy().into_owned())
        });
        let fixed = self.fixed.iter().map(|(var, value)| ((*var).to_owned(), (*value).to_owned()));
        folders.chain(fixed).chain([(ACCOUNT_VAR.to_owned(), account.to_owned())]).collect()
    }

    /// Brings `slot` up to date with the setup in `user_home` (skills, rules, MCP servers, sessions): everything
    /// but the login. Run before each start of the agent on it.
    pub fn share(&self, user_home: &Path, slot: &Path) -> Result<(), Error> {
        for (_, folder) in self.folders {
            slot::make(&slot.join(folder))?;
        }
        slot::share(self.shared, user_home, &user_home.join(self.home), &slot.join(self.mirror))
    }

    /// Whether `slot` is signed in, for an agent whose login is a file in it; `None` for one its CLI is asked
    /// about ([`SignedIn::AgentCheck`], [`SignedIn::Lists`]).
    pub fn signed_in_on_disk(&self, slot: &Path) -> Option<bool> {
        match self.signed_in {
            SignedIn::AgentCheck | SignedIn::Lists { .. } => None,
            SignedIn::File(login) => Some(slot.join(login).is_file()),
            SignedIn::Logins(file) => {
                let logins: Option<serde_json::Value> =
                    std::fs::read(slot.join(file)).ok().and_then(|raw| serde_json::from_slice(&raw).ok());
                Some(logins.as_ref().and_then(serde_json::Value::as_object).is_some_and(|logins| !logins.is_empty()))
            }
        }
    }

    /// Who `slot` is signed in as, where its files say.
    pub fn identity(&self, slot: &Path) -> Option<String> {
        let names = self.names.as_ref()?;
        names.of(&slot.join(names.in_slot))
    }

    /// Who the computer's own login (in `user_home`) is signed in as, where the agent's files say.
    pub fn own_identity(&self, user_home: &Path) -> Option<String> {
        let names = self.names.as_ref()?;
        names.of(&user_home.join(names.own))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_agent_points_its_own_variables_at_the_slot() {
        let slot = Path::new("/slots/a1");
        let env = |id: &str| agent(id).unwrap().env(slot, "a1");
        assert_eq!(
            env("claude"),
            [("CLAUDE_CONFIG_DIR".into(), "/slots/a1".into()), (ACCOUNT_VAR.into(), "a1".into())]
        );
        let opencode = env("opencode");
        assert_eq!(opencode[0], ("XDG_DATA_HOME".into(), slot.join("data").to_string_lossy().into_owned()));
        assert_eq!(opencode[1], ("XDG_STATE_HOME".into(), slot.join("state").to_string_lossy().into_owned()));
        assert_eq!(opencode[2], ("OPENCODE_AUTH_CONTENT".into(), String::new()), "an inline login is cleared");
        assert!(env("antigravity").contains(&("AGY_ACP_FORCE_FILE_STORAGE".into(), "1".into())));
        assert_eq!(env("pi")[0].0, "PI_CODING_AGENT_DIR");
        assert!(env("gemini").contains(&("GEMINI_FORCE_ENCRYPTED_FILE_STORAGE".into(), "false".into())));
        assert!(agent("cursor").is_none());
    }

    #[test]
    fn a_login_kept_in_a_file_says_whether_the_slot_is_signed_in() {
        let slot = slot::tests::scratch();
        let pi = agent("pi").unwrap();
        assert_eq!(pi.signed_in_on_disk(&slot), Some(false), "no logins file yet");
        std::fs::write(slot.join("auth.json"), "{}").unwrap();
        assert_eq!(pi.signed_in_on_disk(&slot), Some(false), "a logins file with none in it");
        std::fs::write(slot.join("auth.json"), r#"{"anthropic": {"type": "oauth"}}"#).unwrap();
        assert_eq!(pi.signed_in_on_disk(&slot), Some(true));
        let devin = agent("devin").unwrap();
        assert_eq!(devin.signed_in_on_disk(&slot), Some(false));
        std::fs::create_dir_all(slot.join("data/devin")).unwrap();
        std::fs::write(slot.join("data/devin/credentials.toml"), "").unwrap();
        assert_eq!(devin.signed_in_on_disk(&slot), Some(true));
        assert_eq!(agent("claude").unwrap().signed_in_on_disk(&slot), None, "its CLI is asked");
    }

    #[test]
    fn a_gemini_slot_is_a_home_whose_gemini_folder_shares_the_users() {
        let root = slot::tests::scratch();
        let (home, slot) = (root.join(".gemini"), root.join("slot"));
        std::fs::create_dir_all(home.join("skills")).unwrap();
        std::fs::write(home.join("oauth_creds.json"), "secret").unwrap();
        std::fs::write(home.join("google_accounts.json"), r#"{"active": "own@example.com"}"#).unwrap();
        let gemini = agent("gemini").unwrap();
        gemini.share(&root, &slot).unwrap();
        assert!(slot.join(".gemini/skills").exists() && slot.join(".gemini/tmp").exists());
        assert!(!slot.join(".gemini/oauth_creds.json").exists(), "its login is its own");
        assert_eq!(gemini.identity(&slot), None);
        assert_eq!(gemini.own_identity(&root).as_deref(), Some("own@example.com"));
    }
}
