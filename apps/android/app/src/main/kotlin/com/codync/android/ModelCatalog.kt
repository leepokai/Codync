package com.codync.android

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import com.codync.android.core.AgentModels
import com.codync.android.core.LinkState
import kotlinx.coroutines.CancellationException

/** Reads an agent login's models: (backend, login profile, read the agent again). */
internal typealias ModelLoad = suspend (String, String?, Boolean) -> AgentModels

/**
 * One agent login's model list as its computer's host has it, for the model pickers (iOS `AgentModelCatalog`): the list
 * the computer gave last ([kept], from `AppState.modelLists`, which the host's `models` events keep current) shows at
 * once, and is asked for again when the link comes back. [retry] (the refresh) asks the computer to read it from the
 * agent again.
 */
internal class ModelCatalog(private val kept: State<AgentModels?>, private var asked: Int) {
    /** The last answer here, for a loader that doesn't keep its lists in the store. */
    private var answer by mutableStateOf<AgentModels?>(null)
    val catalog: AgentModels? get() = kept.value ?: answer
    var loading by mutableStateOf(false)
        private set
    var failure by mutableStateOf<String?>(null)
        private set
    /** Bumped to read the list from the agent again. */
    var retry by mutableIntStateOf(0)

    /** What [load] read last, for [refresh]. */
    private var last: (suspend (Boolean) -> Unit)? = null

    /** [asks]: the refreshes asked for so far; one more than last time reads the agent again. */
    suspend fun load(load: ModelLoad, backend: String, loginProfile: String?, online: Boolean, asks: Int) {
        val refresh = asks != asked
        asked = asks
        last = { read(load, backend, loginProfile, online, it) }
        read(load, backend, loginProfile, online, refresh)
    }

    /** Has the computer read the list from the agent again, returning once it has (pull to refresh). */
    suspend fun refresh() { last?.invoke(true) }

    private suspend fun read(load: ModelLoad, backend: String, loginProfile: String?, online: Boolean, refresh: Boolean) {
        failure = null; loading = false
        if (backend == "custom") return
        if (!online) {
            if (catalog == null) failure = "Connect to this computer to load its models. You can still enter a model ID."
            return
        }
        loading = true
        try { answer = load(backend, loginProfile, refresh) }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { failure = "Couldn't load models: ${error.message ?: "Try again."}" }
        finally { loading = false }
    }
}

/**
 * [backend]'s models for [loginProfile] on [state]'s computer, loaded again when the link comes back, on `retry` or when
 * [reload] (another picker's refresh) changes.
 */
@Composable internal fun rememberModelCatalog(state: AppState, load: ModelLoad, backend: String, loginProfile: String?, reload: Int = 0): ModelCatalog {
    val online = state.actualConnection is LinkState.Ready
    val kept = rememberUpdatedState(state.modelLists[modelListKey(backend, loginProfile)])
    // The list belongs to the computer, the agent and its login profile.
    val models = remember(state.computer.id, backend, loginProfile) { ModelCatalog(kept, reload) }
    val asks = models.retry + reload
    LaunchedEffect(state.computer.id, backend, loginProfile, online, asks) { models.load(load, backend, loginProfile, online, asks) }
    return models
}
