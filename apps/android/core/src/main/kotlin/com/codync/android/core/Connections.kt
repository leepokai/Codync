package com.codync.android.core

import java.net.URI
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.Serializable

/** Public metadata from the existing host marketplace and credential APIs. */
@Serializable data class ComposioStatus(val configured: Boolean, val keyUrl: String)
@Serializable data class ComposioApp(val slug: String, val name: String, val description: String? = null,
    val logo: String? = null, val connection: ComposioConnectionState? = null) {
    val connected: Boolean get() = connection?.status == "active"
}
@Serializable data class ComposioApps(val items: List<ComposioApp>, val nextCursor: String? = null)
@Serializable data class ComposioConnectionState(val id: String, val status: String)
@Serializable data class ComposioField(val name: String, val label: String, val description: String? = null,
    val secret: Boolean = false, val required: Boolean = false)
@Serializable data class ComposioConnect(val status: String, val url: String? = null, val connection: String? = null,
    val mode: String? = null, val fields: List<ComposioField>? = null) {
    fun checked(): ComposioConnect {
        when (status) {
            "redirect" -> {
                require(!connection.isNullOrBlank()) { "The host did not return a connection ID." }
                requireNotNull(url).let(::externalHttps)
            }
            "needsFields" -> {
                require(!mode.isNullOrBlank()) { "The host did not return an authentication mode." }
                val names = fields.orEmpty().map(ComposioField::name)
                require(names.size in 1..64 && names.all(String::isNotBlank) && names.toSet().size == names.size) {
                    "The host returned unsupported connection fields."
                }
            }
            else -> error("This connection method needs a newer app.")
        }
        return this
    }
    fun complete(values: Map<String, String>): Boolean = status == "needsFields" && fields.orEmpty().all {
        !it.required || values[it.name]?.isNotEmpty() == true
    }
    fun submitted(values: Map<String, String>): Map<String, String> {
        checked()
        require(complete(values)) { "Fill in the required connection fields." }
        val known = fields.orEmpty().mapTo(mutableSetOf(), ComposioField::name)
        require(values.keys.all { it in known }) { "Unknown connection field." }
        require(values.values.all { it.toByteArray().size <= 64 * 1024 }) { "A connection field is too large." }
        return values.filterValues(String::isNotEmpty) // Credential whitespace is meaningful.
    }
}
fun externalHttps(value: String): URI {
    require(value.length <= 8_192 && value.none(Char::isISOControl)) { "Invalid sign-in link." }
    val uri = URI(value)
    require(uri.scheme == "https" && !uri.host.isNullOrBlank() && uri.userInfo == null) { "Invalid sign-in link." }
    return uri
}

@Serializable data class SavedLogin(val id: String, val site: String, val username: String = "")
@Serializable data class CredentialStatus(val provider: String, val onePasswordConnected: Boolean = false)
@Serializable data class ConnectionRequest(val kind: String, val status: String, val title: String, val botId: String,
    val toolkit: String? = null, val registryName: String? = null, val connectorId: String? = null,
    val field: String? = null, val location: String? = null, val site: String? = null,
    val backend: String? = null, val logo: String? = null) {
    val canConnect: Boolean get() = status == "pending" && when (kind) {
        "login", "secret" -> true
        "agent" -> !backend.isNullOrBlank()
        "app" -> !toolkit.isNullOrBlank()
        "connection" -> !connectorId.isNullOrBlank() || !registryName.isNullOrBlank()
        else -> false
    }
}

/** Blank replacement fields preserve host values; no operation can reveal an existing value. */
fun credentialReplacements(connector: InstalledConnector, values: Map<String, String>): Map<String, String> {
    require(values.keys.all { it in connector.keys }) { "Unknown credential field." }
    require(values.values.all { it.toByteArray().size <= 64 * 1024 }) { "A credential field is too large." }
    return values.filterValues(String::isNotEmpty)
}

interface AppConnectionTransport {
    suspend fun awaitActive()
    suspend fun start(): ComposioConnect
    suspend fun state(id: String): ComposioConnectionState
    suspend fun submit(mode: String, fields: Map<String, String>): ComposioConnectionState
}
data class AppConnectionState(val loading: Boolean = true, val plan: ComposioConnect? = null,
    val connected: Boolean = false, val saving: Boolean = false, val status: String? = null,
    val connectionId: String? = null, val error: String? = null)

/** An account owns sign-in; a screen may detach for rotation or the external browser. */
class AppConnectionSession(parent: CoroutineScope, private val transport: AppConnectionTransport, resumeId: String? = null) {
    private val job = SupervisorJob(parent.coroutineContext[Job])
    private val scope = CoroutineScope(parent.coroutineContext + job)
    private val mutableState = MutableStateFlow(AppConnectionState())
    val state = mutableState.asStateFlow()
    private var polling: Job? = null
    init {
        if (resumeId != null) {
            require(resumeId.isNotBlank() && resumeId.length <= 1024 && resumeId.none(Char::isISOControl))
            mutableState.update { it.copy(loading = false, connectionId = resumeId, status = "initiated") }
            poll(resumeId) // A cold restore reads the existing attempt; it never starts another one.
        } else {
            scope.launch {
                try {
                    transport.awaitActive()
                    val response = transport.start()
                    val plan = response.checked()
                    mutableState.update { it.copy(loading = false, plan = plan, connectionId = plan.connection) }
                    if (plan.status == "redirect") poll(requireNotNull(plan.connection))
                } catch (cancelled: CancellationException) { throw cancelled }
                catch (_: Exception) { mutableState.update { it.copy(loading = false,
                    error = "Couldn't start sign-in. Check installed apps before trying again.") } }
            }
        }
    }
    private fun poll(id: String) {
        polling?.cancel()
        polling = scope.launch {
            val completed = withTimeoutOrNull(600_000) {
                while (true) {
                    delay(2_000)
                    transport.awaitActive()
                    val response = try { transport.state(id) }
                        catch (cancelled: CancellationException) { throw cancelled }
                        catch (_: Exception) { continue }
                    if (response.id != id) {
                        mutableState.update { it.copy(error = "The host returned a different connection. Close and check installed apps.") }
                        return@withTimeoutOrNull true
                    }
                    if (receive(response)) return@withTimeoutOrNull true
                }
                @Suppress("UNREACHABLE_CODE") false
            }
            if (completed == null) mutableState.update { it.copy(error = "Timed out waiting for sign-in. Check installed apps before trying again.") }
        }
    }
    private fun receive(response: ComposioConnectionState): Boolean {
        if (response.id.isBlank()) {
            mutableState.update { it.copy(error = "The host did not return a connection ID. Check installed apps.") }
            return true
        }
        mutableState.update { it.copy(status = response.status, connectionId = response.id) }
        return when (response.status) {
            "active" -> { mutableState.update { it.copy(connected = true, error = null) }; true }
            "failed", "expired" -> { mutableState.update { it.copy(error = "Sign-in ${response.status}. Close and try again.") }; true }
            else -> false // Unknown statuses retain their actual value and never imply success.
        }
    }
    fun submit(values: Map<String, String>) {
        if (state.value.saving || state.value.connected || !job.isActive) return
        val plan = requireNotNull(state.value.plan)
        val fields = plan.submitted(values)
        mutableState.update { it.copy(saving = true, error = null) }
        scope.launch {
            try {
                transport.awaitActive()
                val response = transport.submit(requireNotNull(plan.mode), fields)
                if (!receive(response)) poll(response.id)
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (_: Exception) { mutableState.update { it.copy(error = "Connection wasn't confirmed. Check installed apps before trying again.") } }
            finally { mutableState.update { it.copy(saving = false) } }
        }
    }
    fun close() { job.cancel() }
}
