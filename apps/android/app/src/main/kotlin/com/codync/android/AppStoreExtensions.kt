package com.codync.android

import java.io.IOException
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

// Extensions on the computer: agent setup, connectors, skills, credentials and Composio apps.

suspend fun AppStore.agentAuth(backend: String): AgentAuth {
    val response = hostQuery("agentAuth", body("backend" to backend), 300_000)
    refreshBackends()
    return WireJson.decodeFromJsonElement(response)
}
suspend fun AppStore.agentAuthenticate(backend: String, method: String): AgentAuth {
    val response = hostQuery("agentAuthenticate", body("backend" to backend, "method" to method), 660_000)
    refreshBackends()
    return WireJson.decodeFromJsonElement(response)
}
suspend fun AppStore.setAgentEnv(backend: String, values: Map<String, String>): AgentAuth {
    val response = hostQuery("setAgentEnv", buildJsonObject { put("backend", backend); put("vars", WireJson.encodeToJsonElement(values)) }, 300_000)
    refreshBackends()
    return WireJson.decodeFromJsonElement(response)
}
internal fun AppStore.setupTerminal(owner: String, backend: String, step: String, method: String?): SetupTerminalSession {
    require(step in listOf("install", "login", "node"))
    return setupTerminals.getOrPut(owner) {
        val generation = state.value.generation
        val transport = object : SetupTerminalTransport {
            var last: EncryptedChannel? = null
            suspend fun connection(): EncryptedChannel {
                check(state.value.generation == generation) { "The active account changed." }
                val active = ready()
                last = active
                return active
            }
            override suspend fun setup(cols: Int, rows: Int): String {
                val active = connection()
                val response = active.call("agentSetup", buildJsonObject { put("backend", backend); put("step", step);
                    method?.let { put("method", it) }; put("cols", cols); put("rows", rows) }, 300_000)
                return response.jsonObject.getValue("term").jsonPrimitive.content
            }
            override suspend fun awaitActive() { foregroundState.first { it } }
            override fun output(term: String): Flow<TerminalEvent> = flow {
                val active = connection()
                emitAll(active.subscribe("term", body("term" to term)).map(TerminalEvent::decode))
            }
            override suspend fun input(term: String, bytes: ByteArray) {
                val active = connection()
                active.call("termInput", body("term" to term, "data" to java.util.Base64.getEncoder().encodeToString(bytes)), 10_000)
            }
            override suspend fun resize(term: String, cols: Int, rows: Int) {
                val active = connection()
                active.call("termResize", buildJsonObject { put("term", term); put("cols", cols); put("rows", rows) }, 10_000)
            }
            override suspend fun close(term: String) { last?.takeIf { it.state.value is LinkState.Ready }?.call("termClose", body("term" to term), 2_000) }
        }
        SetupTerminalSession(contextScope, transport)
    }
}
internal fun AppStore.closeSetupTerminal(owner: String) { setupTerminals.remove(owner)?.close(); refreshBackends() }
fun AppStore.refreshPlugins() {
    if ("plugins" in state.value.busy) { pluginsRefreshAgain = true; return }
    action("plugins") {
        do {
            pluginsRefreshAgain = false
            val connectors = hostQuery("connectors", JsonObject(emptyMap()))
            val skills = hostQuery("skills", JsonObject(emptyMap()))
            val listing = PluginListing(WireJson.decodeFromJsonElement(connectors.jsonObject.getValue("items")),
                WireJson.decodeFromJsonElement(skills.jsonObject.getValue("items")))
            mutableState.update { it.copy(plugins = listing) }
        } while (pluginsRefreshAgain)
    }
}
suspend fun AppStore.marketConnectors(search: String, cursor: String?): MarketConnectors {
    val response = hostQuery("marketConnectors", body("search" to search, "cursor" to cursor.orEmpty()), 60_000)
    return WireJson.decodeFromJsonElement(response)
}
suspend fun AppStore.marketSkills(): List<MarketSkill> {
    val response = hostQuery("marketSkills", JsonObject(emptyMap()), 60_000)
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("items"))
}
suspend fun AppStore.connectorInfo(name: String): MarketConnector {
    val response = hostQuery("connectorInfo", body("registryName" to name), 60_000)
    return WireJson.decodeFromJsonElement(response)
}
suspend fun AppStore.installedConnectors(): List<InstalledConnector> {
    val response = hostQuery("connectors", JsonObject(emptyMap()), 60_000)
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("items"))
}
suspend fun AppStore.verifyConnector(id: String) {
    hostQuery("connectorVerify", body("id" to id), 120_000)
    refreshPlugins()
}
suspend fun AppStore.importConnectors(config: String): List<InstalledConnector> {
    val response = hostQuery("importConnectors", body("config" to config), 60_000)
    refreshPlugins()
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("items"))
}
suspend fun AppStore.credentialStatus(): CredentialStatus {
    val response = hostQuery("credentialStatus", JsonObject(emptyMap()), 60_000)
    return WireJson.decodeFromJsonElement(response)
}
suspend fun AppStore.savedLogins(): List<SavedLogin> {
    val response = hostQuery("credentialLogins", JsonObject(emptyMap()), 60_000)
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("items"))
}
suspend fun AppStore.saveLogin(site: String, username: String, password: String): SavedLogin {
    val response = hostQuery("credentialSaveLogin", body("site" to site, "username" to username, "password" to password), 60_000)
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("login"))
}
suspend fun AppStore.removeLogin(id: String) { hostQuery("credentialRemoveLogin", body("id" to id), 60_000) }
suspend fun AppStore.onePasswordToken(token: String): CredentialStatus {
    val response = hostQuery("credentialSetOnePassword", body("token" to token), 60_000)
    return WireJson.decodeFromJsonElement(response)
}
suspend fun AppStore.updateConnectorCredentials(connector: InstalledConnector, values: Map<String, String>) {
    val fields = credentialReplacements(connector, values)
    require(fields.isNotEmpty()) { "Enter a value to replace." }
    hostQuery("credentialUpdateConnector", buildJsonObject { put("id", connector.id); put("fields", WireJson.encodeToJsonElement(fields)) }, 120_000)
    refreshPlugins()
}
suspend fun AppStore.composioStatus(): ComposioStatus {
    val response = hostQuery("composioStatus", JsonObject(emptyMap()), 60_000)
    return WireJson.decodeFromJsonElement(response)
}
suspend fun AppStore.composioKey(key: String): ComposioStatus {
    val response = hostQuery("setComposioKey", body("key" to key), 60_000)
    refreshPlugins()
    return WireJson.decodeFromJsonElement(response)
}
suspend fun AppStore.composioApps(search: String, cursor: String?): ComposioApps {
    val response = hostQuery("composioToolkits", body("search" to search, "cursor" to cursor.orEmpty()), 60_000)
    return WireJson.decodeFromJsonElement(response)
}
internal fun AppStore.appConnection(owner: String, slug: String, resumeId: String? = null): AppConnectionSession = appConnections.getOrPut(owner) {
    val generation = state.value.generation
    val transport = object : AppConnectionTransport {
        override suspend fun awaitActive() {
            combine(foregroundState, state) { visible, current ->
                visible && current.generation == generation && current.actualConnection is LinkState.Ready
            }.first { it }
        }
        override suspend fun start(): ComposioConnect {
            val response = hostQuery("composioConnect", body("toolkit" to slug), 60_000, generation)
            return WireJson.decodeFromJsonElement(response)
        }
        override suspend fun state(id: String): ComposioConnectionState {
            val response = hostQuery("composioConnection", body("id" to id), 30_000, generation)
            return WireJson.decodeFromJsonElement(response)
        }
        override suspend fun submit(mode: String, fields: Map<String, String>): ComposioConnectionState {
            val response = hostQuery("composioConnectFields", buildJsonObject { put("toolkit", slug); put("mode", mode); put("fields", WireJson.encodeToJsonElement(fields)) }, 60_000, generation)
            return WireJson.decodeFromJsonElement(response)
        }
    }
    AppConnectionSession(contextScope, transport, resumeId)
}
internal fun AppStore.closeAppConnection(owner: String) { appConnections.remove(owner)?.close(); refreshPlugins() }
fun AppStore.completeConnectionRequest(entryId: String, connectorId: String? = null, value: String? = null,
    username: String? = null, cancel: Boolean = false) = action("connection:$entryId") {
    try {
        finishConnectionRequest(entryId, connectorId, value, username, cancel)
        if (cancel) cancelOAuthPending(entryId)
    } catch (cancelled: CancellationException) { throw cancelled }
    catch (_: Exception) { throw IOException(if (cancel) "Cancellation wasn't confirmed. Reconnect and check this request."
        else "Connection wasn't confirmed. Check its setup and refresh before trying again.") }
}
suspend fun AppStore.finishConnectionRequest(entryId: String, connectorId: String? = null, value: String? = null,
    username: String? = null, cancel: Boolean = false, expectedGeneration: Int = state.value.generation) {
    // Own the API reply and its mirror write together; retirement joins both.
    val request = contextScope.async {
        check(state.value.generation == expectedGeneration) { "The active account changed." }
        val active = ready()
        check(state.value.generation == expectedGeneration) { "The active account changed." }
        val payload = buildJsonObject { put("entryId", entryId); put("cancel", cancel)
            connectorId?.let { put("connectorId", it) }; value?.let { put("value", it) }; username?.let { put("username", it) } }
        val response = active.call("connectorRequestFinish", payload, 120_000)
        val entry = Entry.decode(response.jsonObject.getValue("entry").jsonObject)
        withContext(Dispatchers.IO) { requireNotNull(database).receive(listOf(entry)) }
        refresh()
    }
    try { request.await() } finally { request.cancel() }
}
fun AppStore.refreshBackends() = action("backends") {
    val response = hostQuery("refreshBackends", JsonObject(emptyMap()), 60_000)
    mutableState.update { it.copy(hello = JsonObject(it.hello.orEmpty() + ("backends" to response.jsonObject.getValue("backends")))) }
}
suspend fun AppStore.installConnector(item: MarketConnector, option: InstallOption, inputs: Map<String, String>): InstalledConnector {
    require(option in item.options && option.complete(inputs)) { "Complete the required connector fields." }
    val response = hostQuery("installConnector", buildJsonObject { put("registryName", item.name); put("option", option.id); put("inputs", WireJson.encodeToJsonElement(inputs)) }, 45_000)
    val connector = WireJson.decodeFromJsonElement<InstalledConnector>(response.jsonObject.getValue("connector"))
    refreshPlugins()
    return connector
}
fun AppStore.pluginAction(method: String, payload: JsonObject, done: () -> Unit = {}) = action("plugin") {
    require(method in listOf("installConnector", "importConnectors", "removeConnector", "installSkill", "removeSkill", "connectorSignOut", "credentialUpdateConnector"))
    hostQuery(method, payload, 120_000)
    refreshPlugins()
    done()
}
