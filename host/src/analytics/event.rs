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
    /// The watchdog aborted the last host process, which had stopped answering, and the service started this one.
    HostRestartedStuck,
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
    RemoteScreenPaused,
    ImagePreviewOpened,
    ImagePreviewAction,
    ConversationFindOpened,
    ConversationFindSearched,
    QuoteOpened,
    BotCreated,
    BotUpdated,
    BotsReordered,
    SectionRenamed,
    ComputerRenamed,
    BotDeleted,
    MessageSent,
    /// The user had a queued message handled now (`agent::bot::steer`).
    MessageSteered,
    /// The user sent one of the agent's own `/` commands (`agent::bot::commands`).
    AgentCommandSent,
    /// A bot's new Codex session was put in Codex's own Codync section, or wasn't (`agent::bot::codex_section`).
    CodexSessionFiled,
    QueuedMessageRemoved,
    TurnStopped,
    TurnFinished,
    /// A turn moved on to the bot's next agent (`agent::fallback`).
    AgentFellBack,
    /// The user moved a bot with a conversation to another login: its next message carries a recap
    /// (`agent::bot::fallback::handoff`).
    AgentSwitchedByUser,
    /// An agent taking over a bot's chat was told what came before (`agent::fallback::carry`).
    HandoffCarried,
    /// A bot's agent stopped to give back what it held (`agent::bot::lifecycle`).
    AgentRetired,
    /// A bot's own folder became a git repository that keeps its versions (`agent::versions`).
    OwnFolderVersioned,
    /// Saving a version of a bot's own folder failed (`stage`).
    OwnFolderSaveFailed,
    /// Codync started summarizing a bot's chat (`agent::bot::compaction`).
    ContextCompacted,
    UsageReadFailed,
    /// An agent's model list read from the agent (`agent::auth::model_cache`): `background` when a kept list answered.
    ModelListFetched,
    SessionReset,
    PermissionAnswered,
    DecisionAsked,
    DecisionAnswered,
    /// A choice card stopped waiting unanswered (`reason`: `chat::decision::Expiry`).
    DecisionExpired,
    /// The user's choice was saved but its tool call had already stopped waiting.
    DecisionFailed,
    /// An agent's own question (`elicitation/create`) got its reply (`action`; why it was declined: `reason`).
    ElicitationReplied,
    ReactionAdded,
    HandoffAnswered,
    RoutineSaved,
    RoutineToggled,
    RoutineDeleted,
    RoutineRun,
    RoutineRetried,
    /// A bot chain stopped at its handoff limit and left the user a Continue notice (`via`: `chat::team::limit::Via`).
    ChainLimitReached,
    /// The user continued a chain stopped at the handoff limit (`continueChain`).
    ChainContinued,
    /// A bot's `save_progress` ended (`outcome`).
    RoutineProgressSaved,
    /// A bot's `follow_up` ended (`outcome`).
    FollowUpSet,
    /// A follow-up came due and its bot took it.
    FollowUpFired,
    /// A follow-up was dropped before it began (`by`).
    FollowUpCancelled,
    /// A bot's `add_task` ended (`outcome`).
    WorkAdded,
    /// A task of a bot's box ended (`outcome`).
    WorkFinished,
    /// A thread called a tool only its bot's chat may call (`tool`): one its tool list still offered it.
    ThreadToolRefused,
    /// A task was moved, cut in, replaced, stopped, dropped or tried again (`how`), by the bot or the user (`by`).
    WorkSteered,
    /// At host start, worktrees no task knew any more went, their work merged (`removed`).
    WorktreesTidied,
    /// Projects were added to a bot (`count`), by any client or tool (`hub::projects`).
    BotProjectAdded,
    /// Projects were taken off a bot (`count`).
    BotProjectRemoved,
    /// At host start, once: bots that worked in a project moved to their own folder, the project kept (`count`).
    BotProjectsMigrated,
    /// A task's turn couldn't go on until the user signs in to its agent, so it waits for them (`agent`, `source`).
    WorkNeedsSignIn,
    /// A thread ended its turn while what it started in the background still ran, so its task waits on it, once
    /// (`agent`, `source`); `work_finished`'s `held` says how it then ended (#634).
    WorkHeld,
    /// A bot's `tell_task` finished (`outcome`): where the words went (`to`) or why not (`error`), and `now`.
    WorkTold,
    /// A bot's `ask_task` finished (`outcome`): the thread answered in time, later, or the question was refused.
    TaskAsked,
    /// Filling a new worktree failed at one file or folder (`stage`: `include`, `link`); the thread starts anyway.
    WorktreeFillFailed,
    /// A bot's `recall_task` finished (`outcome`), looking up one task, a query or the latest (`by`).
    WorkRecalled,
    /// Threads that last ended longer ago than the user keeps them were deleted (`count`, `days`) (`work::archive`).
    ThreadsDeleted,
    /// An app listed a page of older threads (`count`, `more`, `first`: the first page) (`olderThreads`).
    OlderThreadsListed,
    McpClientCreated,
    McpClientRevoked,
    McpCall,
    McpThreadLaunched,
    /// The user wrote to a task's thread (`chat::work::thread`): whether it reopened the task, and whether it waits.
    WorkMessaged,
    /// The watchdog found one of a bot's own tasks looking stuck (`signal`) and woke the bot, reminded the user of a
    /// card, or let it be (`action`, `reason`) (`chat::work::watch`).
    WatchdogFlagged,
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
    /// A viewer's screen session ended (`screen::watch_viewer`).
    RemoteScreenFinished,
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
    /// A manager bot's `create_bot` or `update_bot`.
    TeamManaged,
    /// A bot's `publish_artifact`.
    ArtifactPublished,
    /// A bot's `send_file` with text: a message that carries a voice note.
    VoiceNoteSent,
    /// A voice note played from its start, or failed to (`outcome`).
    VoiceNotePlayed,
    /// A bot's `send_message` or `send_file` with `reply_to` (`tool`), quoted or sent without it (`outcome`).
    BotQuoted,
    CredentialSaved,
    /// Every saved credential deleted, for a computer whose key is gone (`outcome`).
    CredentialsReset,
    AgentSetupStarted,
    /// A bot's terminal opened (or shown again) in its folder.
    BotTerminalOpened,
    AgentSignedIn,
    /// A login profile was added, renamed or removed, or got its keys (`action`); never its name, who it signs in or
    /// its variables.
    LoginProfileChanged,
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
    /// A Ko-fi invitation (desktop and terminal UI only; at 3, 30 and 90 active days) was shown; on the desktop, then
    /// opened or dismissed.
    KofiInviteShown,
    KofiInviteAnswered,
    /// An important release's highlights were on screen (docs/features/whats-new.md; desktop and terminal UI here,
    /// phones send their own).
    WhatsNewShown,
    /// The desktop's details panel was split into two panes or joined, and took the conversation's place or gave
    /// it back; a page was opened in its Browser tab.
    PanelSplit,
    PanelMaximized,
    PreviewOpened,
    /// Group by computer or Sort by status was switched on the desktop's Bots or Threads list.
    ListLayoutChanged,
    /// The desktop app's system notifications were turned on or off (Settings → General → System).
    DesktopNotificationsSet,
    /// A desktop system notification was clicked, opening its chat.
    DesktopNotificationOpened,
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
    /// A coding agent's session used its Network mailbox (`SessionAction`, with the agent's kind).
    NetworkSession,
    /// `codync-host network setup` set up (or removed) one agent (`reported::SetupOutcome`).
    NetworkSetup,
    /// Error tracking (`errors`): a panic or an uncaught error.
    Exception,
}

impl Event {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::AppOpened => "app_opened",
            Self::WindowOpened => "window_opened",
            Self::HostStarted => "host_started",
            Self::HostRestartedStuck => "host_restarted_stuck",
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
            Self::RemoteScreenPaused => "remote_screen_paused",
            Self::ImagePreviewOpened => "image_preview_opened",
            Self::ImagePreviewAction => "image_preview_action",
            Self::ConversationFindOpened => "conversation_find_opened",
            Self::ConversationFindSearched => "conversation_find_searched",
            Self::QuoteOpened => "quote_opened",
            Self::BotCreated => "bot_created",
            Self::BotUpdated => "bot_updated",
            Self::BotsReordered => "bots_reordered",
            Self::SectionRenamed => "section_renamed",
            Self::ComputerRenamed => "computer_renamed",
            Self::BotDeleted => "bot_deleted",
            Self::MessageSent => "message_sent",
            Self::MessageSteered => "message_steered",
            Self::AgentCommandSent => "agent_command_sent",
            Self::CodexSessionFiled => "codex_session_filed",
            Self::QueuedMessageRemoved => "queued_message_removed",
            Self::TurnStopped => "turn_stopped",
            Self::TurnFinished => "turn_finished",
            Self::AgentFellBack => "agent_fell_back",
            Self::AgentSwitchedByUser => "agent_switched_by_user",
            Self::HandoffCarried => "handoff_carried",
            Self::AgentRetired => "agent_retired",
            Self::OwnFolderVersioned => "own_folder_versioned",
            Self::OwnFolderSaveFailed => "own_folder_save_failed",
            Self::ContextCompacted => "context_compacted",
            Self::UsageReadFailed => "usage_read_failed",
            Self::ModelListFetched => "model_list_fetched",
            Self::SessionReset => "session_reset",
            Self::PermissionAnswered => "permission_answered",
            Self::DecisionAsked => "decision_asked",
            Self::DecisionAnswered => "decision_answered",
            Self::DecisionExpired => "decision_expired",
            Self::DecisionFailed => "decision_failed",
            Self::ElicitationReplied => "elicitation_replied",
            Self::ReactionAdded => "reaction_added",
            Self::HandoffAnswered => "handoff_answered",
            Self::RoutineSaved => "routine_saved",
            Self::RoutineToggled => "routine_toggled",
            Self::RoutineDeleted => "routine_deleted",
            Self::RoutineRun => "routine_run",
            Self::RoutineRetried => "routine_retried",
            Self::ChainLimitReached => "chain_limit_reached",
            Self::ChainContinued => "chain_continued",
            Self::RoutineProgressSaved => "routine_progress_saved",
            Self::FollowUpSet => "follow_up_set",
            Self::FollowUpFired => "follow_up_fired",
            Self::FollowUpCancelled => "follow_up_cancelled",
            Self::WorkAdded => "work_added",
            Self::WorkFinished => "work_finished",
            Self::ThreadToolRefused => "thread_tool_refused",
            Self::WorkSteered => "work_steered",
            Self::WorktreesTidied => "worktrees_tidied",
            Self::BotProjectAdded => "bot_project_added",
            Self::BotProjectRemoved => "bot_project_removed",
            Self::BotProjectsMigrated => "bot_projects_migrated",
            Self::WorkNeedsSignIn => "work_needs_sign_in",
            Self::WorkHeld => "work_held",
            Self::WorkTold => "work_told",
            Self::TaskAsked => "task_asked",
            Self::WorktreeFillFailed => "worktree_fill_failed",
            Self::WorkRecalled => "work_recalled",
            Self::ThreadsDeleted => "threads_deleted",
            Self::OlderThreadsListed => "older_threads_listed",
            Self::McpClientCreated => "mcp_client_created",
            Self::McpClientRevoked => "mcp_client_revoked",
            Self::McpCall => "mcp_call",
            Self::McpThreadLaunched => "mcp_thread_launched",
            Self::WorkMessaged => "work_messaged",
            Self::WatchdogFlagged => "watchdog_flagged",
            Self::DevServerStarted => "dev_server_started",
            Self::DevServerStopped => "dev_server_stopped",
            Self::DevServerAction => "dev_server_action",
            Self::DevServerDetected => "dev_server_detected",
            Self::VoiceCallStarted => "voice_call_started",
            Self::VoiceCallFinished => "voice_call_finished",
            Self::VoiceCallQuality => "voice_call_quality",
            Self::WindowRecovery => "window_recovery",
            Self::RemoteScreenOpened => "remote_screen_opened",
            Self::RemoteScreenFinished => "remote_screen_finished",
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
            Self::TeamManaged => "team_managed",
            Self::ArtifactPublished => "artifact_published",
            Self::VoiceNoteSent => "voice_note_sent",
            Self::VoiceNotePlayed => "voice_note_played",
            Self::BotQuoted => "bot_quoted",
            Self::CredentialSaved => "credential_saved",
            Self::CredentialsReset => "credentials_reset",
            Self::AgentSetupStarted => "agent_setup_started",
            Self::BotTerminalOpened => "bot_terminal_opened",
            Self::AgentSignedIn => "agent_signed_in",
            Self::LoginProfileChanged => "login_profile_changed",
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
            Self::PanelSplit => "panel_split",
            Self::PanelMaximized => "panel_maximized",
            Self::PreviewOpened => "preview_opened",
            Self::ListLayoutChanged => "list_layout_changed",
            Self::DesktopNotificationsSet => "desktop_notifications_set",
            Self::DesktopNotificationOpened => "desktop_notification_opened",
            Self::CloudKeeperChecked => "cloud_keeper_checked",
            Self::CloudSleepAsked => "cloud_sleep_asked",
            Self::JoinFromPhoneAnswered => "join_from_phone_answered",
            Self::DiscoveryFailed => "discovery_failed",
            Self::HostUpdateInstalled => "host_update_installed",
            Self::NetworkJoined => "network_joined",
            Self::NetworkChanged => "network_changed",
            Self::NetworkFailed => "network_failed",
            Self::NetworkSession => "network_session",
            Self::NetworkSetup => "network_setup",
            Self::Exception => "$exception",
        }
    }
}
