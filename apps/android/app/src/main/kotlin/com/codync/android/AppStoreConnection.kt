package com.codync.android

import android.os.Build
import java.io.IOException
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.*

// Pairing, the encrypted link to the computer and the mirror's event stream.

fun AppStore.setRoute(route: ConnectionRoute) = action("route") {
    changeComputer { it.copy(route = route) } ?: return@action
    reconnect().join()
}

/**
 * Changes the paired computer's latest copy, saved and then shown, one change at a time: a switch
 * flipped while a link's `hello` runs isn't undone by the copy that link saves.
 */
internal suspend fun AppStore.changeComputer(change: (Computer) -> Computer): Computer? = computerChanges.withLock {
    val changed = change(state.value.computers.firstOrNull() ?: return@withLock null)
    withContext(Dispatchers.IO) { local.saveComputers(listOf(changed)) }
    mutableState.update { it.copy(computers = listOf(changed)) }
    changed
}

/**
 * Drop the link and dial again at once (iOS `restartStream`), not after the backoff. It runs in the
 * account's scope, so a caller that goes away can't leave the link down.
 */
fun AppStore.reconnect(): Job = contextScope.launch { connectionJob?.cancelAndJoin(); connect() }

/** A pull to refresh in a chat: reconnect, and hold until the link is back or has failed (the header says why). */
suspend fun AppStore.refreshConnection() {
    reconnect().join()
    withTimeoutOrNull(20_000) { state.first { it.actualConnection !is LinkState.Connecting } }
}

fun AppStore.setForeground(active: Boolean) {
    foreground = active
    foregroundState.value = active
    PushDelivery.foreground = transportRequired
    if (active && !state.value.loading) {
        connect()
        acknowledgeVisible()
        refreshCloud()
    }
    if (!active) {
        pairingJob?.cancel()
        if (!transportRequired) {
            connectionJob?.cancel()
            graceJob?.cancel()
            channel?.close()
        }
    }
}

/** Pairs with the computer in a `codync://pair` link: scanned or pasted ([PairedVia.Qr]) or opened from outside ([PairedVia.Link]). */
internal fun AppStore.pair(text: String, via: PairedVia) {
    if (pairingJob?.isActive == true) return
    pairingJob = contextScope.launch {
        state.first { !it.loading }
        mutableState.update { it.copy(pairing = true, error = null) }
        try {
            val pairing = Pairing.parse(text, BuildConfig.URL_SCHEME)
            val old = state.value.computers.firstOrNull()
            require(old == null || old.id == pairing.computer.id) { "Remove this phone's computer in Computer management to pair a different one." }
            require(old == null || old.signKey == pairing.computer.signKey) { "This computer's identity changed. Start over before pairing again." }
            val identity = withContext(Dispatchers.IO) { local.identity() }
            // Picked on this phone: re-pairing keeps them (iOS `AccountStore.pair`).
            val paired = HostConnector.pair(pairing, identity, Build.MODEL.take(100))
            val computer = paired.keepingDevicePicks(old)
            withContext(Dispatchers.IO) { local.saveComputers(listOf(computer)); local.completeSetup() }
            mutableState.update { it.copy(computers = listOf(computer), setupComplete = true) }
            Analytics.capture(AnalyticsEvent.ComputerPaired(via, computer.id))
            Analytics.completeOnboarding(OnboardingVia.Paired)
            if (database == null) openMirror()
            connect()
        } catch (error: CancellationException) { throw error }
        catch (error: Exception) {
            Analytics.capture(AnalyticsEvent.PairingFailed(PairingFailure.of(error)))
            showError(error)
        }
        finally { mutableState.update { it.copy(pairing = false) } }
    }
}

fun AppStore.connect() {
    if (connectionJob?.isActive == true || !transportRequired) return
    val computer = state.value.computers.firstOrNull() ?: return
    val previous = connectionJob
    connectionJob = contextScope.launch {
        previous?.cancelAndJoin()
        var backoff = 1_000L
        while (transportRequired) {
            var connected: EncryptedChannel? = null
            try {
                showConnection(LinkState.Connecting)
                val identity = withContext(Dispatchers.IO) { local.identity() }
                val target = state.value.computers.firstOrNull { it.id == computer.id } ?: return@launch
                val active = HostConnector.connect(target, identity)
                connected = active
                channel = active
                coroutineScope {
                    launch {
                        active.mailbox.collect { event ->
                            val nonce = event["nonce"]?.jsonPrimitive?.content ?: return@collect
                            val send = state.value.pending.firstOrNull { it.nonce == nonce } ?: return@collect
                            val status = if (event["t"]?.jsonPrimitive?.content == "mbox.delivered") SendStatus.Delivering else SendStatus.Failed
                            savePending(send.copy(status = status))
                        }
                    }
                    launch {
                        active.state.collectLatest { link ->
                            showConnection(link)
                            if (link is LinkState.ComputerOffline) {
                                reconcileMailbox(active)
                            }
                            if (link is LinkState.Ready) {
                                val fromHost = HostClient.hello(active, state.value.computers.firstOrNull { it.id == target.id } ?: target)
                                val hello = active.call("hello").jsonObject
                                // This phone's picks (color, route, push) may have changed during those calls.
                                changeComputer { current -> if (current.id == fromHost.id) fromHost.keepingDevicePicks(current) else current }
                                val dropped = withContext(Dispatchers.IO) {
                                    requireNotNull(database).setStamp(requireNotNull(hello["hostId"]?.jsonPrimitive?.content),
                                        BuildConfig.VERSION_NAME + "/" + hello["version"]?.jsonPrimitive?.content)
                                }
                                // Dropped chats page their earlier messages again.
                                mutableState.update { it.copy(historyComplete = if (dropped) emptySet() else it.historyComplete) }
                                noteHello(hello)
                                // The event catch-up is ordered by revision; bot revisions can
                                // follow hundreds of historical entries. Load the current roster
                                // without moving the event cursor before consuming that stream.
                                if (state.value.bots.isEmpty()) {
                                    val roster = HostClient.bots(active)
                                    if (BuildConfig.DEBUG) android.util.Log.d("CodyncSync", "Initial roster: ${roster.size} bots")
                                    withContext(Dispatchers.IO) { requireNotNull(database).receive(roster) }
                                }
                                mutableState.update { it.copy(rosterLoaded = true) }
                                refresh()
                                backoff = 1_000
                                reconcileMailbox(active)
                                acknowledgeVisible()
                                syncPush()
                                while (isActive) {
                                    if (state.value.mismatch != null) { awaitUpdate(active); continue }
                                    val cursor = withContext(Dispatchers.IO) { requireNotNull(database).revision() }
                                    try {
                                        consumeEvents(active, cursor)
                                    } catch (_: Resync) { /* The cursor remains the last committed event. */ }
                                }
                            }
                        }
                    }
                    val end = active.state.first { it is LinkState.Failed } as LinkState.Failed
                    val status = end.status
                    if (status != null) throw HostError(status, end.message)
                    throw IOException(end.message)
                }
            } catch (error: CancellationException) { throw error }
            catch (error: Exception) {
                if (BuildConfig.DEBUG) android.util.Log.d("CodyncSync", "Sync failed: ${error.javaClass.simpleName}")
                showConnection(LinkState.Failed(error.message ?: "Can't reach the computer.", (error as? HostError)?.status))
                if (error is SecurityException || error is HostError && error.status in listOf(403, 426)) return@launch
            } finally {
                connected?.close()
                if (channel === connected) channel = null
            }
            delay(backoff)
            backoff = minOf(backoff * 2, 20_000)
        }
    }
}

/** Drain available catch-up in bounded transactions, without delaying a lone live event. */
private suspend fun AppStore.consumeEvents(active: EncryptedChannel, cursor: Long): Unit = coroutineScope {
    val source = active.events(cursor).produceIn(this)
    try {
        while (isActive) {
            val batch = mutableListOf<MirrorEvent>()
            var next: JsonObject? = source.receiveCatching().getOrThrow()
            var resync = false
            while (next != null && batch.size < 64) {
                when (val event = decode(next)) {
                    null -> {}
                    MirrorEvent.Resync -> { resync = true; break }
                    else -> batch.add(event)
                }
                next = if (batch.size < 64) source.tryReceive().getOrNull() else null
            }
            withContext(Dispatchers.IO) { requireNotNull(database).apply(batch) }
            refresh()
            receiveArtifacts(batch)
            receiveNetwork(batch)
            hearCalls(batch)
            acknowledgeVisible(readAfter(batch, state.value.readingChats))
            if (resync) throw Resync()
        }
    } finally { source.cancel() }
    throw IOException("The event subscription ended.")
}

/** What an event this app can't decode leads to, as on iOS: never skip data unread while fetching everything again can help. */
internal enum class Undecodable { Rewind, AwaitUpdate, Skip }

internal fun undecodable(alreadyRewound: Boolean, hostNewer: Boolean): Undecodable = when {
    !alreadyRewound -> Undecodable.Rewind
    // A newer host this app can't follow: stop syncing until one side updates.
    hostNewer -> Undecodable.AwaitUpdate
    else -> Undecodable.Skip
}

/** An event, or null when it can't be read and is passed over; a rewind or a wait restarts the stream. */
private suspend fun AppStore.decode(event: JsonObject): MirrorEvent? {
    try { return MirrorEvent.decode(event) }
    catch (error: Exception) {
        if (BuildConfig.DEBUG) android.util.Log.d("CodyncSync", "Undecodable event: ${error.javaClass.simpleName}")
        val version = state.value.hello?.text("version").orEmpty()
        when (undecodable(rewoundFor == version, AppVersion.isBelow(BuildConfig.VERSION_NAME, version))) {
            Undecodable.Rewind -> {
                rewoundFor = version
                withContext(Dispatchers.IO) { requireNotNull(database).rewind() }
                throw Resync()
            }
            Undecodable.AwaitUpdate -> { mutableState.update { it.copy(unreadable = true) }; throw Resync() }
            Undecodable.Skip -> { mutableState.update { it.copy(unreadable = true) }; return null }
        }
    }
}

/**
 * Reached, but nothing to sync until one side updates: ask again every 30 s (a host update restarts
 * the link anyway), every 5 s while an update this phone asked for runs, so the card goes soon.
 */
private suspend fun AppStore.awaitUpdate(active: EncryptedChannel) {
    delay(if (state.value.hostUpdate.updating(System.currentTimeMillis())) 5_000 else 30_000)
    noteHello(active.call("hello").jsonObject)
}

/** A different host version may read fine: forget that its data was unreadable and any update asked of the old one (iOS `noteHostVersion`). */
private fun AppStore.noteHello(hello: JsonObject) = mutableState.update {
    val same = it.hello?.text("version") == hello.text("version")
    it.copy(hello = hello, unreadable = it.unreadable && same, hostUpdate = it.hostUpdate.takeIf { same })
}

private fun AppStore.showConnection(link: LinkState) {
    mutableState.update { it.copy(actualConnection = link) }
    val immediate = link is LinkState.Ready || link is LinkState.Failed && link.status in listOf(403, 426)
    if (immediate) {
        graceJob?.cancel()
        mutableState.update { it.copy(connection = link) }
    } else if (graceJob?.isActive != true) {
        val grace = if (state.value.connection is LinkState.Ready) 5_000L else 1_000L
        graceJob = contextScope.launch {
            delay(grace)
            mutableState.update { it.copy(connection = it.actualConnection) }
        }
    }
}

private suspend fun AppStore.reconcileMailbox(active: EncryptedChannel) {
    if (active.route != HostRoute.Relay || state.value.pending.none { it.status in listOf(SendStatus.Waiting, SendStatus.Delivering) }) return
    try {
        val response = active.queued()
        val held = response.getValue("items").jsonArray.associate { it.jsonObject.getValue("nonce").jsonPrimitive.content to it.jsonObject.getValue("state").jsonPrimitive.content }
        for (send in state.value.pending.filter { it.status in listOf(SendStatus.Waiting, SendStatus.Delivering) }) {
            savePending(send.copy(status = when (held[send.nonce]) {
                "queued" -> SendStatus.Waiting
                "delivering" -> SendStatus.Delivering
                else -> SendStatus.Failed
            }))
        }
    } catch (error: CancellationException) { throw error }
    catch (_: IOException) { /* Reconcile when the relay connection recovers. */ }
}

/** The host asked for the stream to start over from the mirror's cursor (or the mirror rewound). */
private class Resync : IOException()
