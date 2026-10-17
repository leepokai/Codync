package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withTimeout

/**
 * `withTimeout` that fails like any other error: a `TimeoutCancellationException` is a
 * cancellation, so the usual `catch (CancellationException) { throw }` would end the work
 * silently. Cancellation by the caller's owner still cancels.
 */
suspend fun <T> deadline(milliseconds: Long, message: String, operation: suspend CoroutineScope.() -> T): T {
    try { return withTimeout(milliseconds, operation) }
    catch (error: TimeoutCancellationException) {
        currentCoroutineContext().ensureActive()
        throw IOException(message, error)
    }
}
