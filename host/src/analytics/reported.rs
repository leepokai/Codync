//! `track`: the app-level events the desktop app and terminal UI report. Only the catalog's
//! events and values get through (docs/reference/analytics-events.md): the host reads the body
//! into these types and builds the properties from them, so no client text can slip in.

use super::{Event, capture};
use crate::hub::Hub;
use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A `track` body: which app reports, and what.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Older desktop builds don't say.
    #[serde(default)]
    pub app: App,
    #[serde(flatten)]
    pub event: Reported,
}

/// The computer's app that reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum App {
    #[default]
    Desktop,
    Tui,
    #[serde(other)]
    Other,
}

/// What `track` accepts, each with exactly its catalog properties; other keys are dropped.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Reported {
    DevServerAction {
        action: DevServerAction,
    },
    AppOpened {
        /// Opened by the login item (desktop).
        #[serde(skip_serializing_if = "Option::is_none")]
        at_login: Option<bool>,
    },
    WindowOpened,
    OnboardingCompleted {
        #[serde(skip_serializing_if = "Option::is_none")]
        signed_in: Option<bool>,
    },
    ComputerAccessFinished {
        /// Screen recording and control granted.
        ready: bool,
        onboarding: bool,
    },
    SignInStarted {
        provider: Provider,
    },
    SignInFailed {
        provider: Provider,
        reason: SignInFailure,
    },
    SignedIn {
        #[serde(skip_serializing_if = "Option::is_none")]
        provider: Option<Provider>,
    },
    SignedOut,
    ContinuedWithoutAccount,
    StarPromptAnswered {
        ask: Ask,
        starred: bool,
    },
    LinkOpened {
        link: Link,
    },
    ScreenViewed {
        screen: Screen,
    },
    /// A picture opened full size (desktop).
    ImagePreviewOpened,
    ImagePreviewAction {
        action: ImageAction,
        from: ImageFrom,
        outcome: ImageOutcome,
        /// Why a copy or download failed.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<ImageFailure>,
    },
    /// Find in a conversation opened (desktop ⌘F, terminal `/` or ^f).
    ConversationFindOpened,
    ConversationFindSearched {
        /// Whether the search found anything; never the text searched for.
        found: bool,
    },
    VoiceCallStarted {
        engine: Engine,
    },
    VoiceCallQuality {
        engine: Engine,
        /// Milliseconds until the engine was ready; left out when it never was.
        #[serde(skip_serializing_if = "Option::is_none")]
        ready_ms: Option<u64>,
        reconnects: u32,
        stuck: VoiceStuck,
        failure: VoiceFailure,
    },
    WindowRecovery {
        outcome: WindowRecoveryOutcome,
    },
    ReachAccountAnswered {
        outcome: ReachOutcome,
        /// Why adding the computer failed.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<ClaimFailure>,
    },
    JoinOfferAnswered {
        outcome: JoinOutcome,
    },
    CloudKeySaved {
        outcome: CloudKeyOutcome,
        /// Older desktop builds don't say.
        #[serde(default)]
        provider: CloudProvider,
    },
    CloudComputerCreated {
        outcome: CloudCreateOutcome,
        provider: CloudProvider,
    },
    CloudOfferOpened {
        provider: CloudProvider,
    },
    CloudComputerRenamed {
        outcome: CloudRenameOutcome,
    },
    CloudComputerDeleted {
        outcome: CloudDeleteOutcome,
        provider: CloudProvider,
    },
    CloudKeyRemoved {
        outcome: CloudKeyRemoveOutcome,
        provider: CloudProvider,
    },
    CloudSnapshotsDeleted {
        outcome: CloudDeleteOutcome,
        provider: CloudProvider,
    },
    KofiInviteShown,
    KofiInviteAnswered {
        opened: bool,
    },
    WhatsNewShown,
}

// Value lists. A value a newer client adds arrives as `other` instead of being refused.

/// What came of a desktop window whose page crashed: it loaded again, or crashed too often and stays down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowRecoveryOutcome {
    Reloaded,
    GaveUp,
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Apple,
    Google,
    Github,
    Hosted,
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignInFailure {
    Cancelled,
    Timeout,
    Verification,
    Error,
    #[serde(other)]
    Other,
}

/// Which time the GitHub star sheet asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ask {
    First,
    Second,
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    Github,
    Docs,
    Issues,
    Discord,
    AppStore,
    Demo,
    Tailscale,
    Privacy,
    Support,
    InstallCommand,
    #[serde(other)]
    Other,
}

/// What was done in the full-size picture viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageAction {
    Zoom,
    CopyImage,
    CopyName,
    Download,
    #[serde(other)]
    Other,
}

/// Where it was done: inside the full-size viewer, or from a picture's menu in a chat or the Library.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFrom {
    Viewer,
    Menu,
    #[serde(other)]
    Other,
}

/// How a picture action ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageOutcome {
    Done,
    Failed,
    #[serde(other)]
    Other,
}

/// Why a picture copy or download failed: its bytes weren't there, couldn't be converted, the clipboard refused them, or the transfer broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFailure {
    Unavailable,
    Convert,
    Clipboard,
    Transfer,
    #[serde(other)]
    Other,
}

/// One screen, sheet or tab, named the same on every client that has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[expect(clippy::enum_variant_names, reason = "the catalog names the remote screen `remote_screen`")]
pub enum Screen {
    Bots,
    State,
    Chat,
    Trace,
    BotDetails,
    Memory,
    Routines,
    Marketplace,
    Settings,
    Computers,
    Usage,
    Account,
    Pairing,
    VoiceSettings,
    RemoteScreen,
    ComputerAccess,
    Help,
    Library,
    Artifact,
    Network,
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    OnDevice,
    Openai,
    Gemini,
    Cloud,
    #[serde(other)]
    Other,
}

/// The first phase of a call's engine that lasted too long.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceStuck {
    None,
    Connecting,
    Reconnecting,
    Speaking,
    #[serde(other)]
    Other,
}

/// Why a call's engine gave up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceFailure {
    None,
    Microphone,
    Connection,
    Provider,
    #[serde(other)]
    Other,
}

/// How the ask before Reach from anywhere turns on ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReachOutcome {
    Added,
    OwnRelay,
    Cancelled,
    Failed,
    #[serde(other)]
    Other,
}

/// How the ask to add this computer to the signed-in account ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinOutcome {
    Added,
    KeptOut,
    Failed,
    #[serde(other)]
    Other,
}

/// How giving Codync Cloud a provider's key ended: kept, turned down (the user was told what to fix), or failed otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudKeyOutcome {
    Saved,
    Refused,
    Failed,
    #[serde(other)]
    Other,
}

/// How creating a Codync Cloud computer ended: created, turned down (over the limit, or the provider refused the key or
/// the plan, and the user was told what to fix), or failed otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudCreateOutcome {
    Created,
    Refused,
    Failed,
    #[serde(other)]
    Other,
}

/// How renaming a Codync Cloud computer ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudRenameOutcome {
    Renamed,
    Failed,
    #[serde(other)]
    Other,
}

/// How deleting a Codync Cloud computer, or its snapshots, ended: deleted, turned down by the cloud (`409`: its snapshots
/// while it sleeps), or failed otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudDeleteOutcome {
    Deleted,
    Refused,
    Failed,
    #[serde(other)]
    Other,
}

/// How removing a cloud provider's key from Codync Cloud ended: removed, turned down by the cloud (`409`: a computer
/// still runs on that provider), or failed otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudKeyRemoveOutcome {
    Removed,
    Refused,
    Failed,
    #[serde(other)]
    Other,
}

/// The cloud provider a Codync Cloud key or computer is for (the cloud's id for it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudProvider {
    Boat,
    Instabox,
    #[default]
    #[serde(other)]
    Other,
}

/// What the apps do with a dev server on their own (the host's methods report the rest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevServerAction {
    Open,
    Copy,
    #[serde(other)]
    Other,
}

/// Why a computer couldn't be added to the account.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimFailure {
    Unreachable,
    Refused,
    Host,
    #[serde(other)]
    Other,
}

impl Track {
    /// The body to send for `event`, as `app` (what the terminal UI posts).
    pub fn body(app: App, event: Reported) -> Value {
        serde_json::to_value(Self { app, event }).expect("plain enums and booleans always serialize")
    }
}

/// `track`: an event the computer's own app reports (app opened, signed in, …).
pub fn track(hub: &Hub, b: &Value) -> Result<()> {
    let (event, properties) = read(b)?;
    capture(hub, event, properties);
    Ok(())
}

/// The event and its properties, rebuilt from the typed body.
fn read(b: &Value) -> Result<(Event, Map<String, Value>)> {
    let track = Track::deserialize(b).context("unknown event")?;
    let Value::Object(mut p) = serde_json::to_value(&track)? else { anyhow::bail!("unknown event") };
    let event = Event::deserialize(p.remove("event").unwrap_or_default()).context("unknown event")?;
    Ok((event, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read_json(b: &Value) -> Option<(Event, Value)> {
        read(b).ok().map(|(e, p)| (e, Value::Object(p)))
    }

    #[test]
    fn an_older_desktop_body_still_counts() {
        assert_eq!(read_json(&json!({"event": "app_opened"})), Some((Event::AppOpened, json!({"app": "desktop"}))));
        assert_eq!(read_json(&json!({"event": "signed_in"})), Some((Event::SignedIn, json!({"app": "desktop"}))));
        assert_eq!(
            read_json(&json!({"event": "onboarding_completed"})),
            Some((Event::OnboardingCompleted, json!({"app": "desktop"})))
        );
        assert_eq!(read_json(&json!({"event": "signed_out"})), Some((Event::SignedOut, json!({"app": "desktop"}))));
    }

    #[test]
    fn only_catalog_events_and_values_get_through() {
        assert!(read(&json!({"event": "message_sent"})).is_err(), "the host reports that one itself");
        assert!(read(&json!({"event": "anything_else"})).is_err());
        assert!(read(&json!({})).is_err());
        assert!(read(&json!({"event": "computer_access_finished", "ready": true})).is_err(), "missing `onboarding`");

        let newer = json!({"event": "screen_viewed", "app": "tui", "screen": "hologram"});
        assert_eq!(read_json(&newer), Some((Event::ScreenViewed, json!({"app": "tui", "screen": "other"}))));
        let unknown_app = json!({"event": "window_opened", "app": "watch"});
        assert_eq!(read_json(&unknown_app), Some((Event::WindowOpened, json!({"app": "other"}))));
        let newer = json!({"event": "cloud_key_removed", "outcome": "expired", "provider": "fly"});
        assert_eq!(
            read_json(&newer),
            Some((Event::CloudKeyRemoved, json!({"app": "desktop", "outcome": "other", "provider": "other"})))
        );
        assert!(read(&json!({"event": "cloud_computer_deleted", "outcome": "deleted"})).is_err(), "missing `provider`");

        // Extra keys, content above all, never reach the event.
        let noisy = json!({"event": "link_opened", "link": "discord", "url": "https://example.com/secret",
            "text": "my prompt", "app": "desktop"});
        let (_, p) = read_json(&noisy).unwrap();
        assert_eq!(p, json!({"app": "desktop", "link": "discord"}));
        let (_, p) = read_json(&json!({"event": "signed_out", "email": "me@example.com"})).unwrap();
        assert_eq!(p, json!({"app": "desktop"}));
    }

    #[test]
    fn every_reported_event_keeps_its_properties() {
        let cases = [
            (json!({"event": "app_opened", "app": "desktop", "at_login": true}), Event::AppOpened),
            (json!({"event": "window_opened", "app": "desktop"}), Event::WindowOpened),
            (
                json!({"event": "onboarding_completed", "app": "desktop", "signed_in": false}),
                Event::OnboardingCompleted,
            ),
            (
                json!({"event": "computer_access_finished", "app": "desktop", "ready": true, "onboarding": false}),
                Event::ComputerAccessFinished,
            ),
            (json!({"event": "sign_in_started", "app": "desktop", "provider": "apple"}), Event::SignInStarted),
            (
                json!({"event": "sign_in_failed", "app": "desktop", "provider": "hosted", "reason": "verification"}),
                Event::SignInFailed,
            ),
            (json!({"event": "signed_in", "app": "desktop", "provider": "google"}), Event::SignedIn),
            (json!({"event": "signed_in", "app": "desktop", "provider": "github"}), Event::SignedIn),
            (json!({"event": "signed_out", "app": "desktop"}), Event::SignedOut),
            (
                json!({"event": "star_prompt_answered", "app": "desktop", "ask": "second", "starred": true}),
                Event::StarPromptAnswered,
            ),
            (json!({"event": "link_opened", "app": "desktop", "link": "install_command"}), Event::LinkOpened),
            (json!({"event": "dev_server_action", "app": "desktop", "action": "open"}), Event::DevServerAction),
            (json!({"event": "screen_viewed", "app": "tui", "screen": "bot_details"}), Event::ScreenViewed),
            (json!({"event": "image_preview_opened", "app": "desktop"}), Event::ImagePreviewOpened),
            (
                json!({"event": "image_preview_action", "app": "desktop", "action": "copy_image", "from": "menu", "outcome": "done"}),
                Event::ImagePreviewAction,
            ),
            (
                json!({"event": "image_preview_action", "app": "desktop", "action": "download", "from": "viewer",
                    "outcome": "failed", "error": "transfer"}),
                Event::ImagePreviewAction,
            ),
            (json!({"event": "conversation_find_opened", "app": "desktop"}), Event::ConversationFindOpened),
            (
                json!({"event": "conversation_find_searched", "app": "desktop", "found": true}),
                Event::ConversationFindSearched,
            ),
            (json!({"event": "voice_call_started", "app": "desktop", "engine": "on_device"}), Event::VoiceCallStarted),
            (
                json!({"event": "voice_call_quality", "app": "desktop", "engine": "gemini", "ready_ms": 1200,
                    "reconnects": 1, "stuck": "none", "failure": "none"}),
                Event::VoiceCallQuality,
            ),
            (json!({"event": "window_recovery", "app": "desktop", "outcome": "reloaded"}), Event::WindowRecovery),
            (json!({"event": "window_recovery", "app": "desktop", "outcome": "gave_up"}), Event::WindowRecovery),
            (json!({"event": "continued_without_account", "app": "desktop"}), Event::ContinuedWithoutAccount),
            (json!({"event": "kofi_invite_shown", "app": "tui"}), Event::KofiInviteShown),
            (json!({"event": "kofi_invite_answered", "app": "desktop", "opened": true}), Event::KofiInviteAnswered),
            (json!({"event": "whats_new_shown", "app": "tui"}), Event::WhatsNewShown),
            (
                json!({"event": "join_offer_answered", "app": "desktop", "outcome": "kept_out"}),
                Event::JoinOfferAnswered,
            ),
            (
                json!({"event": "cloud_key_saved", "app": "desktop", "outcome": "refused", "provider": "instabox"}),
                Event::CloudKeySaved,
            ),
            (
                json!({"event": "cloud_computer_created", "app": "desktop", "outcome": "created", "provider": "boat"}),
                Event::CloudComputerCreated,
            ),
            (json!({"event": "cloud_offer_opened", "app": "desktop", "provider": "instabox"}), Event::CloudOfferOpened),
            (
                json!({"event": "cloud_computer_renamed", "app": "desktop", "outcome": "failed"}),
                Event::CloudComputerRenamed,
            ),
            (
                json!({"event": "cloud_computer_deleted", "app": "desktop", "outcome": "deleted", "provider": "boat"}),
                Event::CloudComputerDeleted,
            ),
            (
                json!({"event": "cloud_key_removed", "app": "desktop", "outcome": "removed", "provider": "instabox"}),
                Event::CloudKeyRemoved,
            ),
            (
                json!({"event": "cloud_snapshots_deleted", "app": "desktop", "outcome": "refused", "provider": "boat"}),
                Event::CloudSnapshotsDeleted,
            ),
            (
                json!({"event": "reach_account_answered", "app": "desktop", "outcome": "own_relay"}),
                Event::ReachAccountAnswered,
            ),
            (
                json!({"event": "reach_account_answered", "app": "desktop", "outcome": "failed", "error": "refused"}),
                Event::ReachAccountAnswered,
            ),
        ];
        for (body, event) in cases {
            let mut expected = body.clone();
            let name = expected.as_object_mut().unwrap().remove("event").unwrap();
            assert_eq!(name, event.name(), "the catalog name");
            assert_eq!(read_json(&body), Some((event, expected)), "{body}");
        }
    }

    #[test]
    fn the_terminal_ui_body_reads_back() {
        let body = Track::body(App::Tui, Reported::ScreenViewed { screen: Screen::Memory });
        assert_eq!(body, json!({"event": "screen_viewed", "app": "tui", "screen": "memory"}));
        assert_eq!(
            Track::body(App::Tui, Reported::AppOpened { at_login: None }),
            json!({"event": "app_opened", "app": "tui"})
        );
    }
}
