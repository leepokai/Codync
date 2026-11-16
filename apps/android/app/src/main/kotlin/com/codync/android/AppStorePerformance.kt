package com.codync.android

import com.codync.android.core.LinkState
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/** Best-effort diagnostics never wait for a reconnect or surface a host error to the reader. */
internal fun AppStore.recordPerformance(measurement: PhonePerformance) {
    if (android.os.Build.HARDWARE in listOf("ranchu", "goldfish")) return
    val active = channel?.takeIf { it.state.value is LinkState.Ready } ?: return
    contextScope.launch {
        try { active.call("recordPhonePerformance", measurement.body()) }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { android.util.Log.i("Codync", "Phone performance was not recorded: ${error.message}") }
    }
}
