package com.codync.android

import android.net.Uri
import androidx.lifecycle.viewModelScope
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

// Push registration, notification settings and the links notifications and widgets open.

fun AppStore.syncPush() = action("push") {
    val active = channel?.takeIf { it.state.value is LinkState.Ready } ?: return@action
    val computer = state.value.computers.firstOrNull() ?: return@action
    try { PushDelivery.sync(getApplication(), local, computer, active) }
    catch (error: CancellationException) { throw error }
    catch (_: Exception) { PushDelivery.schedule(getApplication()) }
}
/** Saved with the computer on this phone (iOS `AccountStore.setNotifications`). */
fun AppStore.notifications(enabled: Boolean) = action("notificationSettings") {
    changeComputer { it.copy(notifications = enabled) } ?: return@action
    Analytics.capture(AnalyticsEvent.SettingChanged(PhoneSetting.Push, enabled))
    if (!enabled) NotificationDisplay.cancel(getApplication(), 1)
    syncPush(); PushDelivery.schedule(getApplication())
}
fun AppStore.taskNotifications(enabled: Boolean) = action("taskSettings") {
    PushPreferences.tasks(getApplication(), enabled)
    mutableState.update { it.copy(tasks = enabled) }
    Analytics.capture(AnalyticsEvent.SettingChanged(PhoneSetting.LiveActivities, enabled))
    if (!enabled) NotificationDisplay.cancel(getApplication(), 2)
    syncPush(); PushDelivery.schedule(getApplication())
}
/** The state once the signed-in (or local) account has loaded: where a link can be opened. */
internal suspend fun AppStore.activated(): AppState = state.first { current -> !current.loading &&
    (current.account.ready || !current.account.configured || current.account.error != null) &&
    current.contextId == LocalStore.referenceId(current.account.userId ?: "local") }

fun AppStore.openLink(uri: Uri) {
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
            if (destination.computerId != activated.computers.firstOrNull()?.id) {
                mutableState.update { it.copy(error = "This notification's computer is no longer paired here.") }
                return@withTimeoutOrNull true
            }
            val generation = activated.generation
            if (usage) {
                if (state.value.generation == generation) {
                    openBot(null)
                    mutableState.update { it.copy(error = null, navigationRequest = it.navigationRequest + 1, navigationPage = LinkPage.Usage) }
                }
                return@withTimeoutOrNull true
            }
            if (activated.bots.none { it.id == destination.botId }) {
                // A cold cache or a newly created bot needs an authoritative current roster.
                // The query belongs to the activated runtime; account retirement joins it.
                val request = contextScope.async {
                    val active = ready()
                    val bots = HostClient.bots(active)
                    withContext(Dispatchers.IO) { requireNotNull(database).receive(bots) }
                    refresh()
                }
                try { request.await() }
                catch (cancelled: CancellationException) { throw cancelled }
                catch (_: Exception) {
                    if (state.value.generation == generation) mutableState.update { it.copy(error = "Reconnect to this computer and try opening the notification again.") }
                    return@withTimeoutOrNull true
                } finally { request.cancel() }
            }
            if (state.value.generation != generation || state.value.contextId != destination.scope) return@withTimeoutOrNull true
            if (state.value.bots.any { it.id == destination.botId && !it.deleted }) {
                mutableState.update { it.copy(error = null, navigationRequest = it.navigationRequest + 1, navigationPage = LinkPage.Bots) }
                openBot(destination.botId)
            }
            else mutableState.update { it.copy(error = "This notification's bot is no longer available on the computer.") }
            true
        }
        if (completed == null) mutableState.update { it.copy(error = "Couldn't open this notification yet. Reconnect and try again.") }
    }
}

/** `codync://computers` and `codync://plugins`: Computer management, or the paired computer's extensions, over the roster. */
fun AppStore.openPage(page: LinkPage) {
    linkJob?.cancel()
    linkJob = viewModelScope.launch {
        val ready = withTimeoutOrNull(30_000) { activated() } ?: return@launch
        if (page == LinkPage.Extensions && ready.computers.isEmpty()) return@launch
        openBot(null)
        mutableState.update { it.copy(navigationRequest = it.navigationRequest + 1, navigationPage = page) }
    }
}
