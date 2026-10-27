package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeout

/** One consumer per direction; both wait for the answer and preserve native candidate order. */
class ScreenSignaling(
    private val scope: CoroutineScope,
    private val events: Flow<ScreenCandidate>,
    private val send: suspend (ScreenCandidate) -> Unit,
    private val apply: suspend (ScreenCandidate) -> Unit,
    private val isMediaConnected: () -> Boolean,
    private val onFailure: (Throwable) -> Unit,
) {
    private enum class State { Active, Detached, Stopped }
    private var state = State.Active
    private val ready = CompletableDeferred<Unit>()
    private val answer = CompletableDeferred<Unit>()
    private val local = Channel<ScreenCandidate>(ScreenCandidate.maxCount + 1)
    private var remoteJob: Job? = null
    private var localJob: Job? = null
    private var failure: Throwable? = null
    private var mediaEstablished = false
    private var localComplete = false
    private var remoteComplete = false
    private var remoteCount = 0

    val isDetached: Boolean get() = state == State.Detached
    val candidatesComplete: Boolean get() = localComplete && remoteComplete

    suspend fun subscribe() {
        remoteJob = scope.launch {
            try {
                events.collect { receive(it) }
                transportEnded(IOException("Screen signaling ended before connecting."))
            } catch (error: CancellationException) {
                if (state == State.Active) fail(error)
            } catch (error: Exception) {
                handle(error)
            }
        }
        try {
            withTimeout(10_000) { ready.await() }
            check()
        } catch (error: TimeoutCancellationException) {
            val timeout = IOException("The screen signaling subscription didn't become ready.", error)
            fail(timeout)
            throw timeout
        } catch (error: CancellationException) {
            stop()
            throw error
        }
    }

    /** WebRTC invokes this on its signaling thread; the bounded channel is the only shared state. */
    fun offerLocal(event: ScreenCandidate) {
        val result = local.trySend(event)
        if (result.isFailure && !result.isClosed) {
            val error = ScreenProtocolException("Too many queued screen ICE candidates.")
            if (local.close(error)) scope.launch { fail(error) }
        }
    }

    fun installedAnswer() {
        check()
        if (state != State.Active) return
        answer.complete(Unit)
        if (localJob != null) throw ScreenProtocolException("The screen answer was already installed.")
        localJob = scope.launch {
            var count = 0
            try {
                for (event in local) {
                    event.validate()
                    if (localComplete || event.type !in listOf(ScreenCandidate.Kind.Candidate, ScreenCandidate.Kind.Complete)) {
                        throw ScreenProtocolException("Screen candidates arrived out of order.")
                    }
                    if (event.type == ScreenCandidate.Kind.Candidate && ++count > ScreenCandidate.maxCount) {
                        throw ScreenProtocolException("Too many screen ICE candidates.")
                    }
                    send(event)
                    if (state != State.Active) return@launch
                    if (event.type == ScreenCandidate.Kind.Complete) localComplete = true
                }
            } catch (error: CancellationException) {
                if (state == State.Active) fail(error)
            } catch (error: Exception) {
                handle(error)
            }
        }
    }

    fun mediaConnected() { if (state == State.Active) mediaEstablished = true }

    fun check() {
        failure?.let { throw it }
        if (state == State.Stopped) throw CancellationException("Screen signaling stopped.")
    }

    private suspend fun receive(event: ScreenCandidate) {
        if (state != State.Active) return
        event.validate()
        when (event.type) {
            ScreenCandidate.Kind.Ready -> {
                if (ready.isCompleted) throw ScreenProtocolException("Duplicate screen subscription readiness.")
                ready.complete(Unit)
            }
            ScreenCandidate.Kind.Error -> throw ScreenProtocolException(event.message ?: "Screen signaling failed.")
            ScreenCandidate.Kind.Candidate, ScreenCandidate.Kind.Complete -> {
                if (!ready.isCompleted || remoteComplete) throw ScreenProtocolException("Screen candidates arrived out of order.")
                if (event.type == ScreenCandidate.Kind.Candidate && ++remoteCount > ScreenCandidate.maxCount) {
                    throw ScreenProtocolException("Too many screen ICE candidates.")
                }
                answer.await()
                if (state != State.Active) return
                if (event.type == ScreenCandidate.Kind.Candidate) {
                    try { apply(event) }
                    catch (error: CancellationException) { throw error }
                    catch (error: Exception) { throw ScreenProtocolException("Couldn't apply a screen ICE candidate.", error) }
                } else remoteComplete = true
            }
        }
    }

    private fun handle(error: Exception) {
        if (error is IOException && error !is ScreenProtocolException && error !is HostError) transportEnded(error)
        else fail(error)
    }

    private fun transportEnded(error: Throwable) {
        if (state != State.Active) return
        if (mediaEstablished || isMediaConnected() || candidatesComplete) {
            state = State.Detached
            finish()
        } else fail(error)
    }

    private fun fail(error: Throwable) {
        if (state != State.Active) return
        failure = error
        stop()
        onFailure(error)
    }

    fun stop() {
        if (state == State.Stopped) return
        state = State.Stopped
        finish()
    }

    private fun finish() {
        ready.completeExceptionally(failure ?: CancellationException("Screen signaling stopped."))
        answer.cancel()
        local.cancel()
        localJob?.cancel()
        remoteJob?.cancel()
    }
}
