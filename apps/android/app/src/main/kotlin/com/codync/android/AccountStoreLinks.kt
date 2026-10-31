package com.codync.android

import android.net.Uri
import android.os.Build
import androidx.lifecycle.viewModelScope
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.sync.withLock

// Pairing and the links notifications, widgets and other apps open.

/** Pairs with the computer in a `codync://pair` link: scanned or pasted ([PairedVia.Qr]) or opened from outside ([PairedVia.Link]). */
internal fun AccountStore.pair(text: String, via: PairedVia) {
    if (pairingJob?.isActive == true) return
    pairingJob = contextScope.launch {
        state.first { !it.loading }
        mutableState.update { it.copy(pairing = true, error = null) }
        try {
            val pairing = Pairing.parse(text, BuildConfig.URL_SCHEME)
            val old = state.value.computers.firstOrNull { it.id == pairing.computer.id }
            require(old == null || old.signKey == pairing.computer.signKey) { "This computer's identity changed. Start over before pairing again." }
            val identity = withContext(Dispatchers.IO) { local.identity() }
            // Picked on this phone: pairing again keeps them (iOS `AccountStore.pair`).
            val computer = HostConnector.pair(pairing, identity, Build.MODEL.take(100)).keepingDevicePicks(old)
            transitions.withLock { addComputer(computer) }
            withContext(Dispatchers.IO) { local.completeSetup() }
            mutableState.update { it.copy(setupComplete = true) }
            Analytics.capture(AnalyticsEvent.ComputerPaired(via, computer.id))
            Analytics.completeOnboarding(OnboardingVia.Paired)
        } catch (error: CancellationException) { throw error }
        catch (error: Exception) {
            Analytics.capture(AnalyticsEvent.PairingFailed(PairingFailure.of(error)))
            showError(error)
        }
        finally { mutableState.update { it.copy(pairing = false) } }
    }
}

/** The state once the signed-in (or local) account has loaded: where a link can be opened. */
internal suspend fun AccountStore.activated(): AccountContext = state.first { current -> !current.loading &&
    (current.account.ready || !current.account.configured || current.account.error != null) &&
    current.contextId == LocalStore.referenceId(current.account.userId ?: "local") }

/** Closes what's open, then shows [page] over the roster. */
private fun AccountStore.navigate(page: LinkPage) {
    openBot(null)
    mutableState.update { it.copy(error = null, navigationRequest = it.navigationRequest + 1, navigationPage = page) }
}

/** A notification's or widget's `codync://bot` or `codync://usage` link: its bot's chat, or that computer's usage. */
fun AccountStore.openLink(uri: Uri) {
    linkJob?.cancel()
    linkJob = viewModelScope.launch {
        val completed = withTimeoutOrNull(30_000) {
            val activated = activated()
            val usage = uri.host == "usage"
            val destination = try {
                if (usage) { val target = UsageDestination.parse(uri.toString(), BuildConfig.URL_SCHEME); BotDestination(target.scope, target.computerId, "") }
                else BotDestination.parse(uri.toString(), BuildConfig.URL_SCHEME)
            }
            catch (_: Exception) { mutableState.update { it.copy(error = "This notification has an invalid destination.") }; return@withTimeoutOrNull true }
            if (destination.scope != activated.contextId) {
                mutableState.update { it.copy(error = "Switch to the account used by this notification to open its bot.") }
                return@withTimeoutOrNull true
            }
            val store = store(destination.computerId)
            if (store == null) {
                mutableState.update { it.copy(error = "This notification's computer is no longer paired here.") }
                return@withTimeoutOrNull true
            }
            val generation = activated.generation
            if (usage) {
                if (state.value.generation == generation) { noteActive(store.id); navigate(LinkPage.Usage) }
                return@withTimeoutOrNull true
            }
            if (store.state.value.bots.none { it.id == destination.botId }) {
                // A cold cache or a newly created bot needs an authoritative current roster.
                // The query belongs to that computer's runtime; retiring it joins the query.
                val request = store.contextScope.async { store.loadRoster() }
                try { request.await() }
                catch (cancelled: CancellationException) { throw cancelled }
                catch (_: Exception) {
                    if (state.value.generation == generation) mutableState.update { it.copy(error = "Reconnect to this computer and try opening the notification again.") }
                    return@withTimeoutOrNull true
                } finally { request.cancel() }
            }
            if (state.value.generation != generation || state.value.contextId != destination.scope) return@withTimeoutOrNull true
            if (store.state.value.bots.any { it.id == destination.botId && !it.deleted }) {
                navigate(LinkPage.Bots)
                store.openBot(destination.botId)
            }
            else mutableState.update { it.copy(error = "This notification's bot is no longer available on the computer.") }
            true
        }
        if (completed == null) mutableState.update { it.copy(error = "Couldn't open this notification yet. Reconnect and try again.") }
    }
}

/**
 * Runs [open] once the account has loaded, in place of any link still waiting; it outlives a context switch on the way
 * (the signed-in account loading after the local one), and a link the account doesn't load for in time is dropped.
 */
private fun AccountStore.whenActivated(open: (AccountContext) -> Unit) {
    linkJob?.cancel()
    linkJob = viewModelScope.launch { withTimeoutOrNull(30_000) { activated() }?.let(open) }
}

/** Text another app shared waits for a bot to go to (`ShareToBot`), once the account has loaded. */
fun AccountStore.share(text: String) = whenActivated {
    navigate(LinkPage.Bots)
    mutableState.update { it.copy(sharing = text) }
}

/** The shared text goes after what's in [to]'s message box, and its chat opens. */
fun AccountStore.sendShared(to: BotDestination) {
    val text = state.value.sharing ?: return
    store(to.computerId)?.appendDraft(to.botId, text)
    openBot(to)
    finishSharing()
}

fun AccountStore.finishSharing() = mutableState.update { it.copy(sharing = null) }

/**
 * The app icon's "Talk to Codync" (`LiveShortcut`): the lead's chat, with a call once its computer is connected, as Open
 * Live Mode without a bot does (`requestCall`); with no lead (deleted, not synced yet), Bots to pick one.
 */
fun AccountStore.talkToLead() = whenActivated { ready ->
    navigate(LinkPage.Bots)
    val (state, lead) = shortcutLead(stores.value.map { it.state.value }, ready.shortcutLeadComputer) ?: return@whenActivated
    val store = store(state.computer.id) ?: return@whenActivated
    store.openBot(lead.id)
    store.requestCall(lead.id)
}

/** `codync://computers` and `codync://plugins`: Computer management, or the last active computer's extensions, over the roster. */
fun AccountStore.openPage(page: LinkPage) = whenActivated { ready ->
    if (page != LinkPage.Extensions || ready.computers.isNotEmpty()) navigate(page)
}
