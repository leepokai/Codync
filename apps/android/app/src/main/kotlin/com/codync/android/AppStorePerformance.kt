package com.codync.android

import com.codync.android.core.LinkState
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/**
 * Emulators skip the host by default, so a UI test's scrolling never writes into a computer's log.
 * An instrumentation test sets this to check that delivery reaches the host (`PhonePerformanceDeliveryTest`).
 */
internal var deliverFromEmulator = false

private val onEmulator = android.os.Build.HARDWARE in listOf("ranchu", "goldfish")

/** Best-effort diagnostics never wait for a reconnect or surface a host error to the reader. */
internal fun AppStore.recordPerformance(measurement: PhonePerformance) {
    if (onEmulator && !deliverFromEmulator) return
    val active = channel?.takeIf { it.state.value is LinkState.Ready } ?: return
    contextScope.launch {
        try { active.call("recordPhonePerformance", measurement.body()) }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { android.util.Log.i("Codync", "Phone performance was not recorded: ${error.message}") }
    }
}

internal fun AppStore.recordOpenTiming(phase: OpenPhase, chats: Int? = null, messages: Int? = null) {
    openTiming.record(phase, id, chats, messages)?.let(::recordPerformance)
}
