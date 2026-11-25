package com.codync.android

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import com.codync.android.core.AgentModels
import com.codync.android.core.LinkState
import kotlinx.coroutines.CancellationException

/** One agent login's model list as its computer's host has it, for the model pickers (iOS `AgentModelCatalog`). */
internal class ModelCatalog {
    var catalog by mutableStateOf<AgentModels?>(null)
        private set
    var loading by mutableStateOf(false)
        private set
    var failure by mutableStateOf<String?>(null)
        private set
    /** Bumped to load the list again. */
    var retry by mutableIntStateOf(0)

    suspend fun load(load: suspend (String, String?) -> AgentModels, backend: String, loginProfile: String?, online: Boolean) {
        failure = null; loading = false
        if (backend == "custom") return
        if (!online) { failure = "Connect to this computer to load its models. You can still enter a model ID."; return }
        loading = true
        try { catalog = load(backend, loginProfile) }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { failure = "Couldn't load models: ${error.message ?: "Try again."}" }
        finally { loading = false }
    }
}

/** [backend]'s models for [loginProfile] on [state]'s computer, loaded again when the link comes back or on `retry`. */
@Composable internal fun rememberModelCatalog(state: AppState, load: suspend (String, String?) -> AgentModels, backend: String,
    loginProfile: String?): ModelCatalog {
    val online = state.actualConnection is LinkState.Ready
    // The list belongs to the computer, the agent and its login profile.
    val models = remember(state.computer.id, backend, loginProfile) { ModelCatalog() }
    LaunchedEffect(state.computer.id, backend, loginProfile, online, models.retry) { models.load(load, backend, loginProfile, online) }
    return models
}
