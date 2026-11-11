package com.codync.android

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.codync.android.core.*
import java.io.IOException
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

/**
 * The ViewModel the UI talks to: the active account's state and the one writer of it. This file holds
 * the account context's lifecycle (load, retire), the mirror refresh and the helpers every feature
 * uses; the features themselves are `AppStore` extensions in the `AppStore*.kt` files beside it.
 */
class AppStore(application: Application) : AndroidViewModel(application) {
    internal val activityOwnerId: String = UUID.randomUUID().toString()
    internal var local = LocalStore(application)
    private var contextJob = SupervisorJob(viewModelScope.coroutineContext[Job])
    internal var contextScope = CoroutineScope(contextJob + Dispatchers.Main.immediate)
    internal val transitions = Mutex()
    /** One change to the paired computer's saved copy at a time (`changeComputer`). */
    internal val computerChanges = Mutex()
    internal val screenTakeovers = Mutex()
    /** Every change to the saved computers goes through `changeComputers`, one at a time. */
    internal val computerWrites = Mutex()
    internal var cloud: CloudClient? = null
    /** The account's word for this device's key, kept in memory for this context. */
    internal var deviceCertificate: DeviceCertificate? = null
    /** Computers joining this account (`joinAccount`), one request each. */
    internal val joining = java.util.Collections.synchronizedSet(mutableSetOf<String>())
    private var sessionJob: Job? = null
    internal var linkJob: Job? = null
    internal val asked = mutableSetOf<String>()
    /** Orders incoming shares: only the newest one may stage itself. */
    internal val shareRequests = java.util.concurrent.atomic.AtomicInteger()
    internal var attachments = AttachmentFiles(application, local.directory)
    internal val downloadLocks = ConcurrentHashMap<String, Mutex>()
    internal val mutableState = MutableStateFlow(AppState())
    val state = mutableState.asStateFlow()
    internal var database: MirrorDatabase? = null
    internal var draftBook: DraftBook? = null
    internal var draftWriter: Job? = null
    internal var savedDrafts: Map<String, ComposerDraft> = emptyMap()
    internal var connectionJob: Job? = null
    internal var pairingJob: Job? = null
    internal var graceJob: Job? = null
    internal var channel: EncryptedChannel? = null
    internal var foreground = false
    internal val foregroundState = MutableStateFlow(false)
    internal val setupTerminals = mutableMapOf<String, SetupTerminalSession>()
    internal val documentModels = mutableMapOf<String, DocumentsModel>()
    internal fun documentsFor(botId: String): DocumentsModel = documentModels.getOrPut(botId) {
        DocumentsModel(botId, this, contextScope, state.value.generation)
    }
    internal val appConnections = mutableMapOf<String, AppConnectionSession>()
    internal var oauthRequestId: String? = null
    internal var voiceBinding: VoiceBinding? = null
    internal var voiceGeneration: Int? = null
    internal val transportRequired: Boolean get() = foreground || state.value.voice != null
    internal val sends = mutableSetOf<String>()
    /** The message sent from here last: its row in the queue scrolls into view ([QueuedMessages]). */
    internal var lastSentNonce: String? = null
    internal val actions = mutableMapOf<String, Job>()
    /** The bots as the mirror last had them; the state shows them with [pendingSectionMoves] laid over (`withMoves`). */
    internal var mirrorBots: List<Bot> = emptyList()
    /** Section moves shown but not answered yet, oldest first. */
    internal val pendingSectionMoves = mutableListOf<SectionMove>()
    /** Queued messages taken back here, by the `rev` they had; the state leaves them out until a newer one (`withRemovals`). */
    internal val pendingRemovals = mutableMapOf<String, Long>()
    internal var pluginsRefreshAgain = false
    internal var messageSpeech: MessageSpeech? = null
    internal var widgetOwner: WidgetOwner? = null
    /** The host version an undecodable event already made the mirror fetch everything again for. */
    internal var rewoundFor: String? = null

    init {
        viewModelScope.launch {
            withContext(Dispatchers.IO) { PushPreferences.splitSwitch(getApplication()) }
            transitions.withLock { loadContext(null) }
            NativeAccounts.observe(viewModelScope) { account ->
                mutableState.update { it.copy(account = account) }
                if (account.ready) transitions.withLock {
                    // Read the latest state after a concurrent logout/reset finishes.
                    val current = state.value.account
                    val desired = current.userId ?: "local"
                    if (desired != local.contextId) {
                        retireContext()
                        loadContext(current.userId)
                    } else if (cloud == null && current.userId != null) {
                        configureCloud(current.userId)
                        refreshCloud()
                    }
                }
            }
        }
    }

    internal suspend fun loadContext(userId: String?) {
        try {
            WidgetRefreshWorker.stop(getApplication())
            local = LocalStore(getApplication(), userId ?: "local")
            widgetOwner = withContext(Dispatchers.IO) { WidgetStore.activate(getApplication(), local.contextId) }
            PushDelivery.activate(getApplication(), userId)
            attachments = AttachmentFiles(getApplication(), local.directory)
            val computers = withContext(Dispatchers.IO) { local.computers() }
            val complete = withContext(Dispatchers.IO) { local.setupComplete() }
            mutableState.update { AppState(contextId = local.referenceId, generation = it.generation + 1,
                account = it.account, computers = computers, setupComplete = complete || computers.isNotEmpty() || userId != null) }
            mutableState.update { it.copy(tasks = PushPreferences.tasks(getApplication())) }
            openMirror()
            if (userId != null) configureCloud(userId)
            mutableState.update { it.copy(loading = false) }
            observeWidgets(requireNotNull(widgetOwner))
            observeExports()
            if (foreground) connect()
            refreshCloud()
        } catch (error: CancellationException) { throw error }
        catch (error: Exception) { showError(error) }
    }

    private suspend fun configureCloud(userId: String) {
        val identity = withContext(Dispatchers.IO) { local.identity() }
        cloud = CloudClient(BuildConfig.CLOUD_URL, identity) { NativeAccounts.token(userId) }
    }

    /**
     * Join every writer before closing a context's database or replacing its identity. `mode` says what
     * leaves: the account (push off the shown computer, every notification gone), the shown computer
     * (its push and notifications only), or nothing but the view (a switch: its tasks stop, alerts stay).
     */
    internal suspend fun retireContext(mode: Retirement = Retirement.Account) = withContext(NonCancellable) {
        mutableState.update { it.copy(loading = true) }
        withContext(Dispatchers.IO) { WidgetStore.retire(getApplication(), widgetOwner) }
        widgetOwner = null
        WidgetRefreshWorker.stop(getApplication())
        try { CodyncWidgets.update(getApplication()) } catch (_: Exception) { }
        retireVoice()
        val writers = actions.values.toList()
        writers.forEach { it.cancel() }; writers.forEach { it.join() }
        val leaving = state.value.computer
        if (mode == Retirement.Account) PushDelivery.retire(getApplication())
        else leaving?.let { NotificationDisplay.cancelComputer(getApplication(), local.referenceId, it.id, tasksOnly = mode == Retirement.Switch) }
        dropExport()
        discardShare()
        val retiringChannel = channel?.takeIf { it.state.value is LinkState.Ready }
        val feed = TaskFeed(local.directory)
        if (retiringChannel != null) withTimeoutOrNull(2_000) {
            try {
                // A switch keeps the computer's alerts coming; its task watches go with the cleared feed.
                if (mode != Retirement.Switch) retiringChannel.call("unregisterDevice")
                for (watch in withContext(Dispatchers.IO) { feed.all() }) retiringChannel.call("unregisterActivity", body("botId" to watch.botId))
            } catch (_: Exception) { }
        }
        // Editing is disabled during retirement; let its last coalesced write finish.
        draftWriter?.join()
        for (documents in documentModels.values.toList()) documents.flushDraft()
        contextJob.cancelAndJoin()
        documentModels.clear()
        setupTerminals.clear()
        appConnections.clear()
        oauthRequestId = null
        AccountImages.retire()
        channel?.close(); channel = null
        connectionJob = null; pairingJob = null; graceJob = null
        actions.clear(); sends.clear(); downloadLocks.clear(); asked.clear(); cloud = null; deviceCertificate = null
        connectionJob = null; graceJob = null
        actions.clear(); sends.clear(); downloadLocks.clear()
        // A cancelled section move never heard back: it would lie over the next mirror for good.
        pendingSectionMoves.clear(); mirrorBots = emptyList()
        pendingRemovals.clear()
        pluginsRefreshAgain = false
        rewoundFor = null
        draftWriter = null; draftBook = null; savedDrafts = emptyMap()
        withContext(Dispatchers.IO) { feed.clear(); database?.close(); database = null }
        withContext(Dispatchers.IO) { ConnectorOAuthBook(local.directory).clear() }
        contextJob = SupervisorJob(viewModelScope.coroutineContext[Job])
        contextScope = CoroutineScope(contextJob + Dispatchers.Main.immediate)
    }

    /** One session change at a time; `wait` runs after the current one instead of being dropped (a switch asked for mid-change). */
    internal fun sessionAction(wait: Boolean = false, operation: suspend () -> Unit) {
        val previous = sessionJob?.takeIf { it.isActive }
        if (previous != null && !wait) return
        sessionJob = viewModelScope.launch {
            previous?.join()
            mutableState.update { it.copy(busy = it.busy + "session", error = null) }
            try { operation() }
            catch (error: CancellationException) { throw error }
            catch (error: Exception) { showError(error) }
            finally { mutableState.update { it.copy(busy = it.busy - "session") } }
        }
    }

    internal suspend fun openMirror() {
        val computer = state.value.computer ?: return
        val book = DraftBook(local.directory, computer.id)
        withContext(Dispatchers.IO) {
            database?.close()
            database = MirrorDatabase(getApplication(), local.directory, computer.id)
            database?.recoverInterrupted()
            val snapshot = requireNotNull(database).snapshot()
            val restored = book.recover(snapshot)
            savedDrafts = restored.associateBy { it.lane }
            mutableState.update { it.copy(drafts = restored.associateBy { draft -> draft.lane }) }
            attachments.retainPending(snapshot.pending.flatMap { it.files } + restored.flatMap { it.files })
        }
        draftBook = book
        refresh()
    }

    internal suspend fun refresh() {
        val mirror = database ?: return
        val snapshot = withContext(Dispatchers.IO) { mirror.snapshot() }
        val retainedDraftFiles = state.value.drafts.values.flatMap { it.files }.map { it.path }.toSet()
        val retired = state.value.pending.filter { old -> snapshot.pending.none { it.nonce == old.nonce } }.flatMap { it.files }.filter { it.path !in retainedDraftFiles }
        withContext(Dispatchers.IO) { retired.forEach(attachments::discard) }
        // Like iOS, a screen state this app can't read is a host without remote screen.
        val screen = snapshot.screen?.let { runCatching { ScreenState.decode(it) }.getOrNull() }
        val visible = snapshot.entries.visibleEntries()
        mutableState.update { current -> current.copy(bots = snapshot.bots, entries = visible,
            pending = snapshot.pending, usage = snapshot.usage, screen = screen, mirrorRevision = snapshot.rev,
        mirrorBots = snapshot.bots
        mutableState.update { current -> current.copy(bots = withMoves(snapshot.bots, pendingSectionMoves), entries = withRemovals(snapshot.entries, pendingRemovals),
            pending = snapshot.pending, usage = snapshot.usage, screen = screen, mirrorRevision = snapshot.rev, icons = snapshot.icons,
            selectedBot = current.selectedBot?.takeIf { id -> snapshot.bots.any { it.id == id } }) }
        if (voiceBinding != null && snapshot.bots.none { it.id == voiceBinding?.botId && !it.deleted }) endVoice("Voice chat ended because its bot was removed.")
        if (PushPreferences.tasks(getApplication())) {
            val feed = TaskFeed(local.directory)
            val watched = withContext(Dispatchers.IO) { feed.all().filter { !it.finished }.map { it.botId }.toSet() }
            for (bot in snapshot.bots.filter { it.id in watched }) {
                val watch = withContext(Dispatchers.IO) { feed.local(bot) }
                if (watch != null) NotificationDisplay.task(getApplication(), local.referenceId, requireNotNull(state.value.computer).id, watch)
            }
        }
    }

    /** The ready channel, waiting briefly for a reconnect; a timeout is an error the action shows. */
    internal suspend fun ready(): EncryptedChannel = deadline(20_000, "Can't reach the computer. Check that it's on, then try again.") {
        // The demo shows sample data: nothing may reach the real computer meanwhile.
        if (DemoMode.active.value) throw IOException(com.codync.android.core.DemoScript.REFUSAL)
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

    internal fun action(key: String, operation: suspend () -> Unit) {
        if (state.value.loading || key in state.value.busy) return
        mutableState.update { it.copy(busy = it.busy + key, error = null) }
        val job = contextScope.launch(start = CoroutineStart.LAZY) {
            try { operation() }
            catch (error: CancellationException) { throw error }
            catch (error: Exception) { showError(error) }
            finally { actions.remove(key); mutableState.update { it.copy(busy = it.busy - key) } }
        }
        actions[key] = job
        job.start()
    }

    internal fun showError(error: Exception) { mutableState.update { it.copy(loading = false, error = error.message ?: "Couldn't complete the action.") } }

    private data class WidgetSource(val computer: Computer?, val bots: List<Bot>, val usage: JsonObject, val fresh: Boolean,
        val approvals: List<WidgetApproval>)
    @OptIn(FlowPreview::class)
    private fun observeWidgets(owner: WidgetOwner) {
        contextScope.launch {
            state.filter { !it.loading && it.contextId == owner.scope }
                .map { WidgetSource(it.computer, it.bots, it.usage, it.widgetBotsCurrent, widgetApprovals(it.entries)) }
                .distinctUntilChanged().debounce(350).collect { source ->
                    try {
                        val usage = UsageReport.decode(source.usage)
                        val accepted = withContext(Dispatchers.IO) { WidgetStore.publish(getApplication(), owner, source.computer, source.bots, usage, source.fresh, false,
                            approvals = if (source.fresh) source.approvals else emptyList()) }
                        if (!accepted) return@collect
                        WidgetRefreshWorker.schedule(getApplication(), source.computer != null)
                        CodyncWidgets.update(getApplication())
                    } catch (cancelled: CancellationException) { throw cancelled }
                    catch (_: Exception) { mutableState.update { it.copy(error = "Couldn't update widgets. Open State and try again.") } }
                }
        }
    }

    override fun onCleared() {
        state.value.voice?.id?.let { VoiceService.stop(getApplication(), it) }
        voiceBinding?.engine?.end()
        messageSpeech?.shutdown()
        channel?.close()
        val old = database
        val retiring = contextJob
        val book = draftBook
        val drafts = state.value.drafts.values.toList()
        val documents = documentModels.values.toList()
        CoroutineScope(Dispatchers.IO).launch {
            retiring.cancelAndJoin()
            try {
                withContext(Dispatchers.Main.immediate) {
                    for (document in documents) document.flushDraft()
                }
                book?.save(drafts)
            }
            catch (_: Exception) { /* An already closing activity cannot report a storage error. */ }
            finally { old?.close() }
        }
        super.onCleared()
    }
}

fun body(vararg values: Pair<String, String?>): JsonObject = buildJsonObject {
    for ((key, value) in values) if (value != null) put(key, value)
}

/** What `retireContext` takes away with the view. */
internal enum class Retirement {
    /** The account leaves this phone (sign-out, switch, start over): its push and every notification. */
    Account,
    /** The shown computer is removed: its push registration and its notifications. */
    Computer,
    /** Another computer is shown instead: only this one's task updates stop; its alerts keep coming. */
    Switch,
}
