package com.codync.android

import com.codync.android.core.*
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

/** Where a link lands once everything open is closed: the roster, or a sheet over it. */
enum class LinkPage { Bots, Usage, Computers, Extensions, Workflows }

/** The workflow stack item between the roster and a chat. A null id is the list; [stepId] is a step run on its own,
 *  outside any run (its push opens it alone). */
data class WorkflowRoute(val id: String? = null, val runId: String? = null, val stepId: String? = null)

/**
 * The active account as the UI renders it (iOS `AccountStore`): its computers, each with its own
 * [AppStore], the account's other computers and what's waiting on them. `AccountStore` owns the only writer.
 */
data class AccountContext(
    val contextId: String = "local",
    val generation: Int = 0,
    val account: AccountState = AccountState(),
    val cloudComputers: List<CloudComputer> = emptyList(),
    /** The account answered (or failed to) at least once, so an empty [cloudComputers] means what it says. */
    val cloudLoaded: Boolean = false,
    /** Access requests waiting for approval on the computer, with the code to check, by computer ID. */
    val access: Map<String, AccessTicket> = emptyMap(),
    /** The request each computer was approved through most recently. */
    val approvedAccess: Map<String, String> = emptyMap(),
    /** Account computers an access request is being sent to. */
    val asking: Set<String> = emptySet(),
    /** The account's Codync Cloud computers and whether it may have them (Computers → Codync Cloud); null until asked. */
    val cloudOffer: CloudOffer? = null,
    /** Codync Cloud sheets open (New cloud computer, a provider's key over it), each showing the account's error itself. */
    val cloudSheets: Int = 0,
    /** Computers → Codync Cloud's card is on screen, showing its cloud computers' errors itself. */
    val cloudCardShown: Boolean = false,
    /** First run: a cloud computer was made from the setup pages, which wait for it (`CloudStartView`) until it's reached. */
    val startedCloud: Boolean = false,
    /** Cloud computers the wait was left with Back: setup doesn't wait for them again (`CloudOffer.starting`). */
    val passedCloud: Set<String> = emptySet(),
    /** The Codync cloud asked this app version to update (`426 appUpdateRequired`), with its reason. */
    val appFloor: String? = null,
    val tasks: Boolean = true,
    val setupComplete: Boolean = false,
    val loading: Boolean = true,
    val pairing: Boolean = false,
    /** While pairing, the code the computer shows if it asks whether to let this phone in. */
    val pairingCheck: String? = null,
    /** The last pairing failed because the phone couldn't reach the computer (not a refusal): the network hint helps. */
    val pairingUnreachable: Boolean = false,
    /** Chose to go on without signing in; signing in or starting over ends it (iOS `withoutAccount`). */
    val withoutAccount: Boolean = false,
    /** Computers asked from here to join this account, with the code they show meanwhile. */
    val addingToAccount: Map<String, String> = emptyMap(),
    /** This phone's computers, in the order it shows them. */
    val computers: List<Computer> = emptyList(),
    /** The computer whose lead "Talk to Codync" calls; null is Automatic (iOS `shortcutLeadComputer`). */
    val shortcutLeadComputer: String? = null,
    /** What another app shared, waiting for a bot to go to. */
    val sharing: SharedContent? = null,
    /** The chats shares went to, most recent first: the share sheet's Recent (iOS `shareRecipients`). */
    val shareRecipients: List<ShareRecipient> = emptyList(),
    /** The open chat's computer was removed: the chat's place shows that until Back (iOS `ChatScreen`). */
    val removedChat: Boolean = false,
    /** The computer the Usage sheet and widget show: the last one a chat was opened or sent on. */
    val lastComputerId: String? = null,
    val navigationRequest: Int = 0,
    val navigationPage: LinkPage = LinkPage.Bots,
    val busy: Set<String> = emptySet(),
    val error: String? = null,
    /** The [AccountStore.action] key [error] came from, when it came from one. */
    val failedAction: String? = null,
) {
    /** The computer Usage acts on: the last active one, else the first. */
    val currentComputerId: String? get() = computers.currentId(lastComputerId)

    /** Codync Cloud runs this account computer: its provider, region and whether it's asleep. */
    internal fun cloudOf(computerId: String?): CloudPlacement? = cloudComputers.firstOrNull { it.computerId == computerId }?.cloud
}

/** One computer as the UI renders it (iOS `BotStore`); its `AppStore` owns the only writer. */
data class AppState(
    val computer: Computer,
    val icons: Map<String, CustomIcon> = emptyMap(),
    val contextId: String = "local",
    val generation: Int = 0,
    val loading: Boolean = true,
    val bots: List<Bot> = emptyList(),
    val rosterLoaded: Boolean = false,
    val mirrorRevision: Long = 0,
    val entries: List<Entry> = emptyList(),
    val pending: List<PendingSend> = emptyList(),
    val drafts: Map<String, ComposerDraft> = emptyMap(),
    val uploadProgress: Map<String, Int> = emptyMap(),
    /** Files bots shared that this phone is downloading, has downloaded or couldn't, by file ID. */
    val fileDownloads: Map<String, FileDownloadState> = emptyMap(),
    val usage: JsonObject = JsonObject(emptyMap()),
    val hello: JsonObject? = null,
    val connection: LinkState = LinkState.Connecting,
    val actualConnection: LinkState = LinkState.Connecting,
    /** The chat open on this computer; at most one computer has one (`AccountStore.openBot`). */
    val selectedBot: String? = null,
    /** A call to start in this bot's chat once it shows ("Talk to Codync"). */
    val callRequest: String? = null,
    /** A bot whose details open over its chat once it shows: a heading or a row of the view of all bots' work. */
    val detailsRequest: String? = null,
    /** A bot and the task of its whose card its chat brings into view once it holds it (a needs-you row of the view of all bots' work). */
    val cardRequest: Pair<String, WorkTask>? = null,
    /** Workflows open between the roster and a chat: the list, or one workflow and an optional run. */
    val workflow: WorkflowRoute? = null,
    val workflows: List<WorkflowSummary> = emptyList(),
    /** False after this host says it has workflows but rejects the methods. */
    val workflowsSupported: Boolean = true,
    /** Shown on the list: deleted, edited elsewhere, or an unknown method. */
    val workflowMessage: String? = null,
    /** Bumps when a workflow event should reload an open detail or run, without touching a dirty graph. */
    val workflowTick: Int = 0,
    val trace: Boolean = false,
    val busy: Set<String> = emptySet(),
    /** Chats that page back no further, and why: their earlier row is dropped. */
    val historyEnd: Map<String, HistoryEnd> = emptyMap(),
    val routines: Map<String, RoutineListing> = emptyMap(),
    /** The computer's artifacts this phone has seen, with the HTML of the ones shown. */
    val artifacts: ArtifactCache = ArtifactCache(),
    /** The agents' model lists this computer gave, by [modelListKey]: a model panel opens on one at once. */
    val modelLists: Map<String, AgentModels> = emptyMap(),
    /** Each chat's Library as last fetched, by chat ID (its artifacts as `artifacts` has them). */
    val library: Map<String, List<LibraryItem>> = emptyMap(),
    /** The transcripts of the threads that are open, by thread id; never part of [entries]. */
    val threadEntries: Map<String, List<Entry>> = emptyMap(),
    /** The user's messages to a thread on their way or refused, by task id, until the thread's transcript has them; in memory only, like the transcript. */
    val threadSends: Map<String, List<PendingSend>> = emptyMap(),
    /** The computer's ended threads past the day its bots' `work` keeps, as far as the Threads list has read them; null until it does. */
    val olderThreads: OlderThreads? = null,
    /** Counts the times [olderThreads] was dropped ([droppingOlderThreads]), so a page asked before is let go (iOS `olderThreadsGeneration`). */
    val olderThreadsGeneration: Int = 0,
    val plugins: PluginListing? = null,
    val voice: VoiceCallViewState? = null,
    /** The computer's remote screen (`null`: the host predates it). */
    val screen: ScreenState? = null,
    /** The computer's network as phones see it (`null`: the host predates it). */
    val network: NetworkView? = null,
    /** The apps given MCP access here, as last read (`null`: not read yet). */
    val mcpApps: List<McpApp>? = null,
    /** The remote screen viewer is open (`codync://screen`, the chat header, Computers). */
    val screenRequest: ScreenRequest? = null,
    /** The host sent data this app can't read even after fetching everything again. */
    val unreadable: Boolean = false,
    /** A sign-in with a code (GitHub): the code, shown before its page opens until Copy and open or Cancel. */
    val signInCode: String? = null,
    /** The update this phone asked of a computer that's behind (`requestHostUpdate`). */
    val hostUpdate: HostUpdate? = null,
    /** Codync Cloud runs this computer (the account's list says, `AccountStore.refreshCloud`): asleep isn't offline. */
    val cloudHosting: CloudPlacement? = null,
    /** The Codync cloud's emergency floor for this app version (`426 appUpdateRequired`), as the account last heard it. */
    val appFloor: String? = null,
    val error: String? = null,
) {
    /** The computer is online and its hello offered workflows. */
    internal val offersWorkflows: Boolean
        get() = workflowsSupported && hello?.get("workflows")?.jsonPrimitive?.booleanOrNull == true
    /** The user can start a thread with a bot here (`startThread`). */
    internal val startsThreads: Boolean get() = hello?.get("startThread")?.jsonPrimitive?.booleanOrNull == true
    /** A thread may start here without a bot, on an agent it names (`threadsWithoutBot`). */
    internal val startsThreadsWithoutBot: Boolean get() = startsThreads && hello?.get("threadsWithoutBot")?.jsonPrimitive?.booleanOrNull == true
    /** A routine here can name the folder its runs work in (`routineFolder`). */
    internal val routineFolders: Boolean get() = hello?.get("routineFolder")?.jsonPrimitive?.booleanOrNull == true
    /** Bots have projects and a worktree mode, and live in their own folder (`hello.projects`). */
    internal val botProjects: Boolean get() = hello?.get("projects")?.jsonPrimitive?.booleanOrNull == true
    /** A bot's git projects can list shared folders, linked into each new worktree (`hello.worktreeFill`). */
    internal val worktreeFill: Boolean get() = hello?.get("worktreeFill")?.jsonPrimitive?.booleanOrNull == true
    /** The computer's name, as its hello gives it. */
    internal val computerName: String get() = hello?.get("name")?.jsonPrimitive?.contentOrNull ?: "Computer"
    /** The computer's home folder, which a folder on it shows as `~`; empty before its hello. */
    /** A thread without a bot gets a folder of its own when none is sent (`hello.threadFolders`). */
    internal val threadFolders: Boolean get() = hello?.get("threadFolders")?.jsonPrimitive?.booleanOrNull == true
    internal val home: String get() = hello?.get("home")?.jsonPrimitive?.contentOrNull.orEmpty()

    /** Connection alone cannot make a cached roster current before event catch-up. */
    internal val widgetBotsCurrent: Boolean get() {
        val head = hello?.get("rev")?.jsonPrimitive?.longOrNull ?: return false
        return !loading && rosterLoaded && actualConnection is LinkState.Ready && mirrorRevision >= head
    }
}

/** Why a chat pages back no further: the computer has no older rows, or paging back no longer grows what the phone shows (its memory budget is spent). */
enum class HistoryEnd { Complete, MemoryFull }
