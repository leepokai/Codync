package com.codync.android

import com.codync.android.core.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

// The computer's network as this phone sees it: read-only, from the stream's hello, kept current by its event.

/**
 * Reads `networkView` again (the sheet opening): the host asks the other computers when what it heard is stale, and
 * the `network` event brings what they answer. A failed read keeps what is shown.
 */
internal fun AppStore.refreshNetwork() {
    val active = channel?.takeIf { it.state.value is LinkState.Ready } ?: return
    contextScope.launch {
        val view = try { NetworkView.decode(active.call("networkView")) }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (error: Exception) { android.util.Log.i("Codync", "Network view failed: ${error.message}"); return@launch }
        mutableState.update { it.copy(network = view) }
    }
}

/** The stream's hello carries the view (null from hosts before it); `network` events change it. */
internal fun AppStore.receiveNetwork(events: List<MirrorEvent>) {
    when (val last = events.lastOrNull { it is MirrorEvent.Hello || it is MirrorEvent.Network }) {
        is MirrorEvent.Hello -> mutableState.update { it.copy(network = last.network) }
        is MirrorEvent.Network -> mutableState.update { it.copy(network = last.view) }
        else -> {}
    }
}
