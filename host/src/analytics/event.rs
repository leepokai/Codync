//! The events the host reports, and the name each one is sent under.

use serde::Deserialize;

/// Everything the host reports. Names follow the `object_verb` convention; each event's
/// sender and properties: docs/reference/analytics-events.md.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    AppOpened,
    WindowOpened,
    HostStarted,
    OnboardingCompleted,
    ComputerAccessFinished,
    SignInStarted,
    SignInFailed,
    SignedIn,
    SignedOut,
    PairingCodeCreated,
    DevicePaired,
    AccessRequestDenied,
    DeviceRevoked,
    PhoneConnected,
    StarPromptAnswered,
    LinkOpened,
    ScreenViewed,
    BotCreated,
    BotUpdated,
    BotsReordered,
    SectionRenamed,
    BotDeleted,
    MessageSent,
    /// The user had a queued message handled now (`agent::bot::steer`).
    MessageSteered,
    QueuedMessageRemoved,
    TurnStopped,
    TurnFinished,
    /// A turn moved on to the bot's next agent (`agent::fallback`).
    AgentFellBack,
    /// A bot's agent stopped to give back what it held (`agent::bot::lifecycle`).
    AgentRetired,
    /// Codync started summarizing a bot's chat (`agent::bot::compaction`).
    ContextCompacted,
    UsageReadFailed,
    SessionReset,
    PermissionAnswered,
    ReactionAdded,
    HandoffAnswered,
    RoutineSaved,
    RoutineToggled,
    RoutineDeleted,
    RoutineRun,
    RoutineRetried,
    /// A bot's `start_dev_server` ended (`outcome`).
    DevServerStarted,
    /// A dev server ended (`by`).
    DevServerStopped,
    /// A person acted on a dev server (`action`).
    DevServerAction,
    /// A bot left a server running through its own shell, not `start_dev_server`.
    DevServerDetected,
    VoiceCallStarted,
    VoiceCallFinished,
    /// How one engine of a call fared (time to ready, reconnects, a phase stuck, failure).
    VoiceCallQuality,
    /// A desktop window's crashed page loaded again, or crashed too often and stays down.
    WindowRecovery,
    RemoteScreenOpened,
    ScreenTakenOver,
    ConnectorInstalled,
    ConnectorsImported,
    BotsImported,
    ConnectorRemoved,
    ConnectorUpdated,
    ConnectorSigninStarted,
    SkillInstalled,
    SkillRemoved,
    SkillWritten,
    /// A bot's `publish_artifact`.
    ArtifactPublished,
    CredentialSaved,
    AgentSetupStarted,
    AgentSignedIn,
    SettingChanged,
    /// The desktop app's ask before Reach from anywhere turns on for a computer in no account.
    ReachAccountAnswered,
    ContinuedWithoutAccount,
    JoinOfferAnswered,
    /// The user gave Codync Cloud the key to their own account with a cloud provider (desktop; phones send their own).
    CloudKeySaved,
    /// The user created a Codync Cloud computer on one of their providers (desktop; phones send their own).
    CloudComputerCreated,
    /// The user opened a cloud provider's free plan from its key sheet (desktop; phones send their own).
    CloudOfferOpened,
    /// The user renamed a Codync Cloud computer (desktop; phones send their own).
    CloudComputerRenamed,
    /// The user deleted a Codync Cloud computer (desktop; phones send their own).
    CloudComputerDeleted,
    /// The user removed their key for a cloud provider from Codync Cloud (desktop; phones send their own).
    CloudKeyRemoved,
    /// The user deleted a Codync Cloud computer's snapshots (desktop; phones send their own).
    CloudSnapshotsDeleted,
    /// The Ko-fi invitation under the first reply (desktop and terminal UI only) was shown; on the desktop, then opened
    /// or dismissed.
    KofiInviteShown,
    KofiInviteAnswered,
    /// An important release's highlights were on screen (docs/features/whats-new.md; desktop and terminal UI here,
    /// phones send their own).
    WhatsNewShown,
    /// A Codync Cloud computer's host started: how its keeper stood, and whether its agents could still use sudo.
    CloudKeeperChecked,
    /// A Codync Cloud computer's host, idle long enough, asked the cloud to put it to sleep (`remote::managed`).
    CloudSleepAsked,
    /// A paired phone asked to add this computer to its account, and the computer answered.
    JoinFromPhoneAnswered,
    /// Advertising on the local network failed, so a computer in no account can't be found at a new address.
    DiscoveryFailed,
    HostUpdateInstalled,
    /// `networkCreate` or `networkJoin`: this computer is in a network of the user's computers.
    NetworkJoined,
    NetworkChanged,
    /// A network step failed (`NetworkStage`): starting a node, a letter, or a `network*` call.
    NetworkFailed,
    /// Error tracking (`errors`): a panic or an uncaught error.
    Exception,
}

impl Event {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::AppOpened => "app_opened",
            Self::WindowOpened => "window_opened",
            Self::HostStarted => "host_started",
            Self::OnboardingCompleted => "onboarding_completed",
            Self::ComputerAccessFinished => "computer_access_finished",
            Self::SignInStarted => "sign_in_started",
            Self::SignInFailed => "sign_in_failed",
            Self::SignedIn => "signed_in",
            Self::SignedOut => "signed_out",
            Self::PairingCodeCreated => "pairing_code_created",
            Self::DevicePaired => "device_paired",
            Self::AccessRequestDenied => "access_request_denied",
            Self::DeviceRevoked => "device_revoked",
            Self::PhoneConnected => "phone_connected",
            Self::StarPromptAnswered => "star_prompt_answered",
            Self::LinkOpened => "link_opened",
            Self::ScreenViewed => "screen_viewed",
            Self::BotCreated => "bot_created",
            Self::BotUpdated => "bot_updated",
            Self::BotsReordered => "bots_reordered",
            Self::SectionRenamed => "section_renamed",
            Self::BotDeleted => "bot_deleted",
            Self::MessageSent => "message_sent",
            Self::MessageSteered => "message_steered",
            Self::QueuedMessageRemoved => "queued_message_removed",
            Self::TurnStopped => "turn_stopped",
            Self::TurnFinished => "turn_finished",
            Self::AgentFellBack => "agent_fell_back",
            Self::AgentRetired => "agent_retired",
            Self::ContextCompacted => "context_compacted",
            Self::UsageReadFailed => "usage_read_failed",
            Self::SessionReset => "session_reset",
            Self::PermissionAnswered => "permission_answered",
            Self::ReactionAdded => "reaction_added",
            Self::HandoffAnswered => "handoff_answered",
            Self::RoutineSaved => "routine_saved",
            Self::RoutineToggled => "routine_toggled",
            Self::RoutineDeleted => "routine_deleted",
            Self::RoutineRun => "routine_run",
            Self::RoutineRetried => "routine_retried",
            Self::DevServerStarted => "dev_server_started",
            Self::DevServerStopped => "dev_server_stopped",
            Self::DevServerAction => "dev_server_action",
            Self::DevServerDetected => "dev_server_detected",
            Self::VoiceCallStarted => "voice_call_started",
            Self::VoiceCallFinished => "voice_call_finished",
            Self::VoiceCallQuality => "voice_call_quality",
            Self::WindowRecovery => "window_recovery",
            Self::RemoteScreenOpened => "remote_screen_opened",
            Self::ScreenTakenOver => "screen_taken_over",
            Self::ConnectorInstalled => "connector_installed",
            Self::ConnectorsImported => "connectors_imported",
            Self::BotsImported => "bots_imported",
            Self::ConnectorRemoved => "connector_removed",
            Self::ConnectorUpdated => "connector_updated",
            Self::ConnectorSigninStarted => "connector_signin_started",
            Self::SkillInstalled => "skill_installed",
            Self::SkillRemoved => "skill_removed",
            Self::SkillWritten => "skill_written",
            Self::ArtifactPublished => "artifact_published",
            Self::CredentialSaved => "credential_saved",
            Self::AgentSetupStarted => "agent_setup_started",
            Self::AgentSignedIn => "agent_signed_in",
            Self::SettingChanged => "setting_changed",
            Self::ReachAccountAnswered => "reach_account_answered",
            Self::ContinuedWithoutAccount => "continued_without_account",
            Self::JoinOfferAnswered => "join_offer_answered",
            Self::CloudKeySaved => "cloud_key_saved",
            Self::CloudComputerCreated => "cloud_computer_created",
            Self::CloudOfferOpened => "cloud_offer_opened",
            Self::CloudComputerRenamed => "cloud_computer_renamed",
            Self::CloudComputerDeleted => "cloud_computer_deleted",
            Self::CloudKeyRemoved => "cloud_key_removed",
            Self::CloudSnapshotsDeleted => "cloud_snapshots_deleted",
            Self::KofiInviteShown => "kofi_invite_shown",
            Self::KofiInviteAnswered => "kofi_invite_answered",
            Self::WhatsNewShown => "whats_new_shown",
            Self::CloudKeeperChecked => "cloud_keeper_checked",
            Self::CloudSleepAsked => "cloud_sleep_asked",
            Self::JoinFromPhoneAnswered => "join_from_phone_answered",
            Self::DiscoveryFailed => "discovery_failed",
            Self::HostUpdateInstalled => "host_update_installed",
            Self::NetworkJoined => "network_joined",
            Self::NetworkChanged => "network_changed",
            Self::NetworkFailed => "network_failed",
            Self::Exception => "$exception",
        }
    }
}
