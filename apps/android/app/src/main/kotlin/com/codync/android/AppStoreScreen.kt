package com.codync.android

import androidx.lifecycle.viewModelScope
import com.codync.android.core.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

// The remote screen viewer: opening it, and taking control from bots.

/** Opens the screen viewer, optionally watching a bot that's using the computer. */
data class ScreenRequest(val watching: String? = null)

fun AppStore.openScreen(watching: String? = null) = mutableState.update { it.copy(screenRequest = ScreenRequest(watching)) }

fun AppStore.closeScreen() = mutableState.update { it.copy(screenRequest = null) }

/** `codync://screen`: the last active computer's screen (iOS `currentStore`), once the account has loaded. */
fun AccountStore.openScreenLink() {
    linkJob?.cancel()
    linkJob = viewModelScope.launch {
        val ready = withTimeoutOrNull(30_000) { activated() } ?: return@launch
        store(ready.currentComputerId)?.openScreen()
    }
}

/** Takes control from bots (they can still look) or hands it back; requests go out in the order they're made. */
fun AppStore.screenTakeover(on: Boolean) {
    contextScope.launch {
        screenTakeovers.withLock {
            try {
                val state = ready().call("screenTakeover", buildJsonObject { put("on", on) }).jsonObject
                withContext(Dispatchers.IO) { database?.apply(MirrorEvent.Screen(state)) }
                refresh()
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (error: Exception) { showError(error) }
        }
    }
}
