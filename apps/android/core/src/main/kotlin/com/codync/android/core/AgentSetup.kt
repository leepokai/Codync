package com.codync.android.core

import java.io.IOException
import java.util.Base64
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.*
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.*

@Serializable data class AgentBackend(val id: String, val name: String, val installed: Boolean = false,
    val available: Boolean = false, val description: String? = null, val installHint: String = "",
    val signedIn: Boolean? = null, val canInstall: Boolean = false, val curated: Boolean = false,
    val registry: String? = null, val node: String? = null,
    /** Its extra logins, for an agent that can have them (null otherwise, and from older hosts). */
    val accounts: List<AgentAccount>? = null) {
    /** Runs on Node.js and the computer has none yet. */
    val needsNode: Boolean get() = node == "missing"
}
/** One more login to an agent, beside the one the computer already has. [name]: who it signs in, once it has. */
@Serializable data class AgentAccount(val id: String, val name: String? = null, val signedIn: Boolean = false)
/** The logins a bot on this agent can pick: the signed-in ones, and the bot's [own] even when it isn't (so its row shows it). */
fun AgentBackend.accountChoices(own: String?): List<AgentAccount> = accounts.orEmpty().filter { it.signedIn || it.id == own }
@Serializable data class AgentAuth(val signedIn: Boolean? = null, val detail: String? = null,
    val methods: List<AgentAuthMethod> = emptyList(), val savedEnv: List<String> = emptyList(), val login: Boolean = false)
@Serializable data class AgentAuthMethod(val id: String, val name: String, val description: String? = null,
    val kind: String? = null, val vars: List<AgentAuthVar> = emptyList(), val link: String? = null) {
    fun complete(values: Map<String, String>, saved: List<String>): Boolean = vars.all {
        it.optional || values[it.name]?.isNotBlank() == true || it.name in saved
    }
}
@Serializable data class AgentAuthVar(val name: String, val label: String, val secret: Boolean = true, val optional: Boolean = false)

sealed interface TerminalEvent {
    data class Output(val bytes: ByteArray) : TerminalEvent
    data class Exit(val code: Int) : TerminalEvent
    companion object {
        fun decode(event: JsonObject): TerminalEvent = when (event.text("type")) {
            "output" -> {
                val encoded = requireNotNull(event.text("data"))
                require(encoded.length <= 350_000) { "Terminal output is too large." }
                val bytes = Base64.getDecoder().decode(encoded)
                require(bytes.size <= 256 * 1024) { "Terminal output is too large." }
                Output(bytes)
            }
            "exit" -> Exit(requireNotNull(event["code"]?.jsonPrimitive?.intOrNull))
            else -> throw IOException("Unknown terminal event.")
        }
    }
}

interface SetupTerminalTransport {
    suspend fun setup(cols: Int, rows: Int): String
    suspend fun awaitActive() {}
    fun output(term: String): Flow<TerminalEvent>
    suspend fun input(term: String, bytes: ByteArray)
    suspend fun resize(term: String, cols: Int, rows: Int)
    suspend fun close(term: String)
}
interface TerminalSink {
    suspend fun write(bytes: ByteArray)
    fun reset()
    fun input(enabled: Boolean)
}
data class SetupTerminalState(val starting: Boolean = true, val connected: Boolean = false,
    val exitCode: Int? = null, val error: String? = null)

/** Screen ownership and account ownership are separate: leaving closes the PTY, rotation replays it. */
class SetupTerminalSession(parent: CoroutineScope, private val transport: SetupTerminalTransport) {
    private val job = SupervisorJob(parent.coroutineContext[Job])
    private val scope = CoroutineScope(parent.coroutineContext + job)
    private val mutableState = MutableStateFlow(SetupTerminalState())
    val state = mutableState.asStateFlow()
    private val started = CompletableDeferred<String>()
    private var term: String? = null
    private var size = 80 to 24
    private var sentSize = size
    private var closed = false
    private var inputFailed = false
    private var queuedBytes = 0
    private val commands = Channel<ByteArray?>(64) // null asks the one ordered sender to synchronize size.
    private var outputJob: Job? = null
    private var inputJob: Job? = null
    private var sink: TerminalSink? = null
    private val startJob = scope.launch {
        try {
            val initial = size
            val id = transport.setup(initial.first, initial.second)
            term = id; sentSize = initial
            if (closed) return@launch
            mutableState.update { it.copy(starting = false) }
            started.complete(id)
            commands.trySend(null)
            inputJob = scope.launch {
                try {
                    for (bytes in commands) {
                        if (size != sentSize) { val next = size; transport.resize(id, next.first, next.second); sentSize = next }
                        if (bytes != null) {
                            try { transport.input(id, bytes) } finally { queuedBytes -= bytes.size; bytes.fill(0) }
                        }
                    }
                } catch (cancelled: CancellationException) { throw cancelled }
                catch (_: Exception) { failInput("Terminal input wasn't confirmed. Leave this terminal and check the computer before starting again.") }
            }
            // Account retirement also owns cleanup, before its channel is removed.
            awaitCancellation()
        } catch (cancelled: CancellationException) { started.cancel(); throw cancelled }
        catch (_: Exception) {
            started.completeExceptionally(IOException("Couldn't start agent setup. Check the computer and try again."))
            mutableState.update { it.copy(starting = false, error = "Couldn't start agent setup. Check the computer and try again.") }
        } finally {
            job.cancel()
            commands.close()
            while (true) { val remaining = commands.tryReceive(); if (remaining.isFailure) break; remaining.getOrNull()?.fill(0) }
            term?.let { closeRemote(it) }
        }
    }

    fun attach(target: TerminalSink) {
        sink?.input(false); outputJob?.cancel(); sink = target
        outputJob = scope.launch {
            val id = try { started.await() } catch (cancelled: CancellationException) { throw cancelled }
                catch (_: Exception) { return@launch }
            var attempts = 0
            while (!closed) {
                try {
                    transport.awaitActive()
                    target.reset(); target.input(!inputFailed && state.value.exitCode == null)
                    mutableState.update { it.copy(connected = true, error = if (inputFailed) it.error else null) }
                    transport.output(id).collect { event ->
                        attempts = 0
                        when (event) {
                            is TerminalEvent.Output -> target.write(event.bytes)
                            is TerminalEvent.Exit -> { target.input(false); mutableState.update { it.copy(exitCode = event.code, connected = false) } }
                        }
                    }
                    if (state.value.exitCode != null) return@launch
                    throw IOException("Terminal stream ended.")
                } catch (cancelled: CancellationException) { throw cancelled }
                catch (error: Exception) {
                    target.input(false); mutableState.update { it.copy(connected = false) }
                    if (error is HostError && error.status == 404 || ++attempts > 5) {
                        mutableState.update { it.copy(error = "This setup terminal is unavailable. Go back and check the agent again.") }; return@launch
                    }
                }
                delay(1_000)
            }
        }
    }
    fun detach(target: TerminalSink) { if (sink === target) { target.input(false); sink = null; outputJob?.cancel(); outputJob = null } }
    fun type(bytes: ByteArray) {
        if (bytes.isEmpty()) return
        if (closed || inputFailed || !state.value.connected || state.value.exitCode != null) { bytes.fill(0); return }
        if (bytes.size > 64 * 1024 || queuedBytes + bytes.size > 256 * 1024) {
            bytes.fill(0); failInput("Terminal input filled its queue. Leave this terminal and check the computer before starting again.")
            return
        }
        queuedBytes += bytes.size
        if (commands.trySend(bytes).isFailure) {
            queuedBytes -= bytes.size; bytes.fill(0)
            failInput("Terminal input filled its queue. Leave this terminal and check the computer before starting again.")
        }
    }
    private fun failInput(message: String) { inputFailed = true; sink?.input(false); inputJob?.cancel(); commands.close(); mutableState.update { it.copy(error = message) } }
    fun resized(cols: Int, rows: Int) { size = cols.coerceIn(20, 500) to rows.coerceIn(4, 500); commands.trySend(null) }
    fun close() {
        if (closed) return
        closed = true; sink?.input(false); sink = null; outputJob?.cancel(); inputJob?.cancel(); commands.close()
        mutableState.update { it.copy(connected = false) }
        // A setup reply can arrive after Back. Keep waiting for its ID so we can close that PTY.
        if (term != null) startJob.cancel()
    }
    suspend fun closeAndJoin() { close(); startJob.cancelAndJoin(); job.cancelAndJoin() }
    private suspend fun closeRemote(id: String) = withContext(NonCancellable) {
        withTimeoutOrNull(2_000) { try { transport.close(id) } catch (_: Exception) {} }
    }
}
