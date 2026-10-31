package com.codync.android

import com.codync.android.core.InstalledConnector
import com.codync.android.core.WireJson
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

// The Extensions calls a sheet waits on (iOS `BotStore+Plugins`): the host does the work, the
// installed list refreshes after, and failures stay on the sheet that asked.

/** A connector you describe yourself: a command line with env, or an https URL with headers. */
internal suspend fun AppStore.addConnector(name: String, command: String?, url: String?, env: Map<String, String>,
    headers: Map<String, String>): InstalledConnector {
    val response = hostCall("installConnector", buildJsonObject {
        put("name", name); command?.let { put("command", it) }; url?.let { put("url", it) }
        put("env", WireJson.encodeToJsonElement(env)); put("headers", WireJson.encodeToJsonElement(headers))
    }, 45_000)
    refreshPlugins()
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("connector"))
}

internal suspend fun AppStore.removePlugin(method: String, id: String) {
    require(method in listOf("removeConnector", "removeSkill"))
    hostCall(method, buildJsonObject { put("id", id) }, 60_000)
    refreshPlugins()
}

internal suspend fun AppStore.installSkill(source: String) {
    hostCall("installSkill", buildJsonObject { put("source", source) }, 60_000)
    refreshPlugins()
}

internal suspend fun AppStore.addSkill(name: String, description: String, instructions: String) {
    hostCall("installSkill", buildJsonObject { put("name", name); put("description", description); put("instructions", instructions) }, 60_000)
    refreshPlugins()
}

/** A skill's `SKILL.md` without its front matter: an installed one by `id`, a catalog one by `source`. */
internal suspend fun AppStore.skillDetails(id: String?, source: String?): String =
    hostCall("skillDetails", buildJsonObject { if (source != null) put("source", source) else put("id", requireNotNull(id)) }, 60_000)
        .jsonObject.getValue("markdown").jsonPrimitive.content

/**
 * Signs in to a connector and waits while the browser finishes it (the store catches the return);
 * false when the store reports a failure or ten minutes pass.
 */
internal suspend fun AppStore.signInConnector(id: String, requestId: String? = null): Boolean {
    clearError()
    connectorSignIn(id, requestId)
    fun signedIn(s: AppState) = s.plugins?.connectors?.any { it.id == id && it.auth == "signedIn" } == true
    val settled = withTimeoutOrNull(600_000) { state.first { signedIn(it) || it.error != null } } ?: return false
    return signedIn(settled)
}

/** The computer's own name (its hello), the one everything here installs on. */
internal val AppState.hostName: String get() = hello?.text("name") ?: computer.name
