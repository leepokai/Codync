package com.codync.android

import android.app.Application
import com.codync.android.core.*
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

/**
 * One computer's store (iOS `BotStore`): its encrypted link, its mirror and the one writer of its [AppState].
 * `AccountStore` makes one for each computer of the account and retires them with it. This file holds the
 * store's lifecycle (open, retire), the mirror refresh and the helpers every feature uses; the features
 * themselves are `AppStore` extensions in the `AppStore*.kt` files beside it.
 */
class AppStore internal constructor(
    internal val application: Application,
    internal val local: LocalStore,
    computer: Computer,
    contextId: String,
    generation: Int,
    parent: Job,
    /** Saves this computer's latest copy among the account's computers. */
    private val saveComputer: suspend (Computer) -> Unit,
    /** A chat opened here: the other computers close theirs (one chat is open at a time). */
    internal val opened: (AppStore) -> Unit,
    /** A usage read finished (the account refreshes the Usage widget). */
    internal val usageRead: suspend (AppStore) -> Unit = {},
) {
    val id: String = computer.id
    private val job = SupervisorJob(parent)
    internal val contextScope = CoroutineScope(job + Dispatchers.Main.immediate)
    /** One change to this computer's saved copy at a time (`changeComputer`). */
    internal val computerChanges = Mutex()
    internal val screenTakeovers = Mutex()
    internal val attachments = AttachmentFiles(application, local.directory)
    internal val downloadLocks = ConcurrentHashMap<String, Mutex>()
    internal val mutableState = MutableStateFlow(AppState(computer, contextId = contextId, generation = generation))
    val state = mutableState.asStateFlow()
    internal val feed = TaskFeed(local.directory, computer.id)
    internal var database: MirrorDatabase? = null
    internal var draftBook: DraftBook? = null
    internal var draftWriter: Job? = null
    internal var savedDrafts: Map<String, ComposerDraft> = emptyMap()
    internal var connectionJob: Job? = null
    internal var graceJob: Job? = null
    internal var channel: EncryptedChannel? = null
    internal var foreground = false
    internal val foregroundState = MutableStateFlow(false)
    internal val setupTerminals = mutableMapOf<String, SetupTerminalSession>()
    internal val appConnections = mutableMapOf<String, AppConnectionSession>()
    internal var oauthRequestId: String? = null
    internal var voiceBinding: VoiceBinding? = null
    internal var voiceGeneration: Int? = null
    /** A "Talk to Codync" call waiting for the link (`requestCall`). */
    internal var callWait: Job? = null
    internal val transportRequired: Boolean get() = foreground || state.value.voice != null
    internal val sends = mutableSetOf<String>()
    internal val actions = mutableMapOf<String, Job>()
    /** The bots as the mirror last had them; the state shows them with [pendingSectionMoves] laid over (`withMoves`). */
    internal var mirrorBots: List<Bot> = emptyList()
    /** Section moves shown but not answered yet, oldest first. */
    internal val pendingSectionMoves = mutableListOf<SectionMove>()
    internal var pluginsRefreshAgain = false
    /** The host version an undecodable event already made the mirror fetch everything again for. */
    internal var rewoundFor: String? = null

    /** Opens the mirror and the drafts this phone keeps for the computer; nothing connects yet. */
    internal suspend fun open() {
        val book = DraftBook(local.directory, id)
        withContext(Dispatchers.IO) {
            database = MirrorDatabase(application, local.directory, id)
            database?.recoverInterrupted()
            val snapshot = requireNotNull(database).snapshot()
            val restored = book.recover(snapshot)
            savedDrafts = restored.associateBy { it.botId }
            mutableState.update { it.copy(drafts = restored.associateBy { draft -> draft.botId }) }
        }
        draftBook = book
        refresh()
        mutableState.update { it.copy(loading = false) }
    }

    /** The files this computer's queued messages and drafts still need, so starting up keeps them. */
    internal fun pendingFiles(): List<OutgoingFile> = state.value.pending.flatMap { it.files } + state.value.drafts.values.flatMap { it.files }

    /** Joins every writer before the account closes this computer's database or forgets it. */
    internal suspend fun retire(unregister: Boolean = true) = withContext(NonCancellable) {
        mutableState.update { it.copy(loading = true) }
        retireVoice()
        val writers = actions.values.toList()
        writers.forEach { it.cancel() }; writers.forEach { it.join() }
        val retiringChannel = channel?.takeIf { it.state.value is LinkState.Ready }
        if (unregister && retiringChannel != null) withTimeoutOrNull(2_000) {
            try {
                retiringChannel.call("unregisterDevice")
                for (watch in withContext(Dispatchers.IO) { feed.all() }) retiringChannel.call("unregisterActivity", body("botId" to watch.botId))
            } catch (_: Exception) { }
        }
        // Editing is disabled during retirement; let its last coalesced write finish.
        draftWriter?.join()
        job.cancelAndJoin()
        setupTerminals.clear()
        appConnections.clear()
        oauthRequestId = null
        channel?.close(); channel = null
        connectionJob = null; graceJob = null
        actions.clear(); sends.clear(); downloadLocks.clear()
        // A cancelled section move never heard back: it would lie over the next mirror for good.
        pendingSectionMoves.clear(); mirrorBots = emptyList()
        pluginsRefreshAgain = false
        rewoundFor = null
        draftWriter = null; draftBook = null; savedDrafts = emptyMap()
        withContext(Dispatchers.IO) { feed.clear(); database?.close(); database = null }
    }

    /** Saves the computer's latest copy (`changeComputer`), then shows it. */
    internal suspend fun save(computer: Computer) {
        saveComputer(computer)
        mutableState.update { it.copy(computer = computer) }
    }

    internal suspend fun refresh() {
        val mirror = database ?: return
        val snapshot = withContext(Dispatchers.IO) { mirror.snapshot() }
        val retainedDraftFiles = state.value.drafts.values.flatMap { it.files }.map { it.path }.toSet()
        val retired = state.value.pending.filter { old -> snapshot.pending.none { it.nonce == old.nonce } }.flatMap { it.files }.filter { it.path !in retainedDraftFiles }
        withContext(Dispatchers.IO) { retired.forEach(attachments::discard) }
        // Like iOS, a screen state this app can't read is a host without remote screen.
        val screen = snapshot.screen?.let { runCatching { ScreenState.decode(it) }.getOrNull() }
        mirrorBots = snapshot.bots
        mutableState.update { current -> current.copy(bots = withMoves(snapshot.bots, pendingSectionMoves), entries = snapshot.entries,
            pending = snapshot.pending, usage = snapshot.usage, screen = screen, mirrorRevision = snapshot.rev, icons = snapshot.icons,
            selectedBot = current.selectedBot?.takeIf { id -> snapshot.bots.any { it.id == id } }) }
        if (voiceBinding != null && snapshot.bots.none { it.id == voiceBinding?.botId && !it.deleted }) endVoice("Voice chat ended because its bot was removed.")
        if (PushPreferences.tasks(application)) {
            val watched = withContext(Dispatchers.IO) { feed.all().filter { !it.finished }.map { it.botId }.toSet() }
            for (bot in snapshot.bots.filter { it.id in watched }) {
                val watch = withContext(Dispatchers.IO) { feed.local(bot) }
                if (watch != null) NotificationDisplay.task(application, local.referenceId, id, watch)
            }
        }
    }

    /** The ready channel, waiting briefly for a reconnect; a timeout is an error the action shows. */
    internal suspend fun ready(): EncryptedChannel = deadline(20_000, "Can't reach the computer. Check that it's on, then try again.") {
        while (true) {
            val active = channel
            if (active?.state?.value is LinkState.Ready) return@deadline active
            val link = active?.state?.value
            if (link is LinkState.ComputerOffline) throw IOException("The computer is offline.")
            if (link is LinkState.Failed && link.status in listOf(403, 426)) throw HostError(requireNotNull(link.status), link.message)
            delay(100)
        }
        @Suppress("UNREACHABLE_CODE") error("Unreachable")
    }

    /** Runs [operation] as [key], its error shown; not while loading or while [key] still runs. Whether it started. */
    internal fun action(key: String, operation: suspend () -> Unit): Boolean {
        if (state.value.loading || key in state.value.busy) return false
        mutableState.update { it.copy(busy = it.busy + key, error = null) }
        val job = contextScope.launch(start = CoroutineStart.LAZY) {
            try { operation() }
            catch (error: CancellationException) { throw error }
            catch (error: Exception) { showError(error) }
            finally { actions.remove(key); mutableState.update { it.copy(busy = it.busy - key) } }
        }
        actions[key] = job
        job.start()
        return true
    }

    internal fun showError(error: Exception) { mutableState.update { it.copy(error = error.message ?: "Couldn't complete the action.") } }

    /** The app is closing: drop the link and save the drafts on the way out. */
    internal fun close(scope: CoroutineScope) {
        state.value.voice?.id?.let { VoiceService.stop(application, it) }
        voiceBinding?.engine?.end()
        channel?.close()
        val old = database
        val book = draftBook
        val drafts = state.value.drafts.values.toList()
        scope.launch {
            job.cancelAndJoin()
            try { book?.save(drafts) }
            catch (_: Exception) { /* An already closing activity cannot report a storage error. */ }
            finally { old?.close() }
        }
    }
}

fun body(vararg values: Pair<String, String?>): JsonObject = buildJsonObject {
    for ((key, value) in values) if (value != null) put(key, value)
}
