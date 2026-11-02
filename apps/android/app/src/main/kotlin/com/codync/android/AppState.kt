package com.codync.android

import com.codync.android.core.*
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

/** Where a link lands once everything open is closed: the roster, or a sheet over it. */
enum class LinkPage { Bots, Usage, Computers, Extensions }

/**
 * The active account as the UI renders it (iOS `AccountStore`): its computers, each with its own
 * [AppStore], the account's other computers and what's waiting on them. `AccountStore` owns the only writer.
 */
data class AccountContext(
    val contextId: String = "local",
    val generation: Int = 0,
    val account: AccountState = AccountState(),
    val cloudComputers: List<CloudComputer> = emptyList(),
    /** Access requests waiting for approval on the computer, with the code to check, by computer ID. */
    val access: Map<String, AccessTicket> = emptyMap(),
    /** The request each computer was approved through most recently. */
    val approvedAccess: Map<String, String> = emptyMap(),
    /** Account computers an access request is being sent to. */
    val asking: Set<String> = emptySet(),
    val tasks: Boolean = true,
    val setupComplete: Boolean = false,
    val loading: Boolean = true,
    val pairing: Boolean = false,
    /** This phone's computers, in the order it shows them. */
    val computers: List<Computer> = emptyList(),
    /** The computer whose lead "Talk to Codync" calls; null is Automatic (iOS `shortcutLeadComputer`). */
    val shortcutLeadComputer: String? = null,
    /** Text another app shared, waiting for a bot to go to. */
    val sharing: String? = null,
    /** The open chat's computer was removed: the chat's place shows that until Back (iOS `ChatScreen`). */
    val removedChat: Boolean = false,
    /** The computer the Usage sheet and widget show: the last one a chat was opened or sent on. */
    val lastComputerId: String? = null,
    val navigationRequest: Int = 0,
    val navigationPage: LinkPage = LinkPage.Bots,
    val busy: Set<String> = emptySet(),
    val error: String? = null,
) {
    /** The computer Usage acts on: the last active one, else the first. */
    val currentComputerId: String? get() = computers.currentId(lastComputerId)
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
    val trace: Boolean = false,
    val busy: Set<String> = emptySet(),
    val historyComplete: Set<String> = emptySet(),
    val routines: Map<String, RoutineListing> = emptyMap(),
    /** The computer's artifacts this phone has seen, with the HTML of the ones shown. */
    val artifacts: ArtifactCache = ArtifactCache(),
    /** Each chat's Library as last fetched, by chat ID (its artifacts as `artifacts` has them). */
    val library: Map<String, List<LibraryItem>> = emptyMap(),
    val plugins: PluginListing? = null,
    val voice: VoiceCallViewState? = null,
    /** The computer's remote screen (`null`: the host predates it). */
    val screen: ScreenState? = null,
    /** The computer's network as phones see it (`null`: the host predates it). */
    val network: NetworkView? = null,
    /** The remote screen viewer is open (`codync://screen`, the chat header, Computers). */
    val screenRequest: ScreenRequest? = null,
    /** The host sent data this app can't read even after fetching everything again. */
    val unreadable: Boolean = false,
    /** The update this phone asked of a computer that's behind (`requestHostUpdate`). */
    val hostUpdate: HostUpdate? = null,
    val error: String? = null,
) {
    /** Connection alone cannot make a cached roster current before event catch-up. */
    internal val widgetBotsCurrent: Boolean get() {
        val head = hello?.get("rev")?.jsonPrimitive?.longOrNull ?: return false
        return !loading && rosterLoaded && actualConnection is LinkState.Ready && mirrorRevision >= head
    }
}
