package com.codync.android

import com.codync.android.core.*
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

/** Everything the UI renders for the active account and its computer; `AppStore` owns the only writer. */
data class AppState(
    val contextId: String = "local",
    val generation: Int = 0,
    val account: AccountState = AccountState(),
    val cloudComputers: List<CloudComputer> = emptyList(),
    val access: AccessTicket? = null,
    val notifications: Boolean = true,
    val tasks: Boolean = true,
    val setupComplete: Boolean = false,
    val loading: Boolean = true,
    val pairing: Boolean = false,
    val computers: List<Computer> = emptyList(),
    val bots: List<Bot> = emptyList(),
    val rosterLoaded: Boolean = false,
    val mirrorRevision: Long = 0,
    val entries: List<Entry> = emptyList(),
    val pending: List<PendingSend> = emptyList(),
    val drafts: Map<String, ComposerDraft> = emptyMap(),
    val uploadProgress: Map<String, Int> = emptyMap(),
    val usage: JsonObject = JsonObject(emptyMap()),
    val hello: JsonObject? = null,
    val connection: LinkState = LinkState.Connecting,
    val actualConnection: LinkState = LinkState.Connecting,
    val selectedBot: String? = null,
    val navigationRequest: Int = 0,
    val navigationUsage: Boolean = false,
    val thread: String? = null,
    val trace: Boolean = false,
    val busy: Set<String> = emptySet(),
    val historyComplete: Set<String> = emptySet(),
    val routines: Map<String, RoutineListing> = emptyMap(),
    val plugins: PluginListing? = null,
    val voice: VoiceCallViewState? = null,
    /** The computer's remote screen (`null`: the host predates it). */
    val screen: ScreenState? = null,
    /** The remote screen viewer is open (`codync://screen`, the thread header, Computers). */
    val screenRequest: ScreenRequest? = null,
    /** The host sent data this app can't read even after fetching everything again. */
    val unreadable: Boolean = false,
    val error: String? = null,
) {
    /** Connection alone cannot make a cached roster current before event catch-up. */
    internal val widgetBotsCurrent: Boolean get() {
        val head = hello?.get("rev")?.jsonPrimitive?.longOrNull ?: return false
        return !loading && rosterLoaded && actualConnection is LinkState.Ready && mirrorRevision >= head
    }
}
