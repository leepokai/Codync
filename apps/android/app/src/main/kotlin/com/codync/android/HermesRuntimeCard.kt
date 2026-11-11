package com.codync.android

import androidx.compose.animation.animateContentSize
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import com.codync.android.core.LinkState
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.serialization.json.*

/** Host-owned Hermes settings, independent of the surrounding bot editor's unsaved draft. */
@Composable internal fun HermesRuntimeCard(botId: String, state: AppState, store: AppStore) {
    val scope = rememberCoroutineScope()
    val model = remember(store, state.generation, botId) { HermesRuntimeModel(botId, store, state.generation, scope) }
    var advanced by remember(model) { mutableStateOf(false) }
    val online = state.actualConnection is LinkState.Ready
    LaunchedEffect(model) { model.load() }
    CardSection("Hermes runtime", "Settings are applied by this computer after the current turn. Provider credentials stay with the existing Hermes profile.",
        modifier = Modifier.animateContentSize(Motion.layoutSize)) {
        if (model.loading && model.config == null) row { Waiting("Loading Hermes settings…") }
        if (model.config != null) {
            row { SwitchRow("Use runtime settings", "Keep the bot's existing Hermes profile and connections.",
                model.enabled, model::enable, enabled = !model.busy) }
            runtimeChoice(model, "Environment", "terminal", "backend", listOf(
                "" to "From Hermes profile", "local" to "Local", "docker" to "Docker", "ssh" to "SSH",
                "modal" to "Modal", "daytona" to "Daytona", "vercel_sandbox" to "Vercel Sandbox"))
            when (model.text("terminal", "backend")) {
                "ssh" -> {
                    runtimeText(model, "SSH host", "terminal", "ssh_host", "Existing SSH host or alias")
                    runtimeText(model, "SSH user", "terminal", "ssh_user", "Existing SSH user")
                    runtimeText(model, "SSH port", "terminal", "ssh_port", "From SSH configuration")
                    runtimeText(model, "SSH key path", "terminal", "ssh_key", "Use the existing SSH configuration")
                }
                "docker" -> runtimeText(model, "Docker image", "terminal", "docker_image", "From Hermes profile")
                "modal" -> runtimeChoice(model, "Modal mode", "terminal", "modal_mode", listOf(
                    "" to "From Hermes profile", "auto" to "Automatic", "managed" to "Managed", "direct" to "Direct"))
                "vercel_sandbox" -> runtimeChoice(model, "Vercel runtime", "terminal", "vercel_runtime", listOf(
                    "" to "From Hermes profile", "node24" to "Node 24", "node22" to "Node 22", "python3.13" to "Python 3.13"))
            }
            runtimeChoice(model, "Memory", null, "memoryProvider", listOf(
                "native" to "From Hermes profile", "builtin" to "Built-in", "honcho" to "Honcho"))
            row { Text("Honcho uses its existing external provider configuration; it is not copied into Codync's memory store.", style = Type.caption, color = Palette.secondary) }
            runtimeText(model, "Delegation provider", "delegation", "provider", "From Hermes profile")
            runtimeText(model, "Delegation model", "delegation", "model", "From Hermes profile")
            runtimeText(model, "Maximum iterations", "delegation", "max_iterations", "From Hermes profile (1–200)")
            runtimeText(model, "Concurrent children", "delegation", "max_concurrent_children", "From Hermes profile (1–10)")
            runtimeText(model, "Child inactivity timeout (seconds)", "delegation", "child_timeout_seconds", "From Hermes profile (30–3600)")
            row { Text("Saved task routes let the agent choose a provider and model per task. Each child keeps the limits above.", style = Type.caption, color = Palette.secondary) }
            model.routes.forEach { value ->
                val route = value as? JsonObject
                val id = route?.get("id")?.jsonPrimitive?.contentOrNull
                if (route != null && id != null) {
                    for ((field, label) in listOf("label" to "Route name", "provider" to "Route provider", "model" to "Route model")) {
                        row { Field(label) { FieldBox(route[field]?.jsonPrimitive?.contentOrNull.orEmpty(),
                            { if (!model.busy) model.editRoute(id, field, it) }, "", capitalize = false) } }
                    }
                    row { SecondaryButton("Remove route", enabled = !model.busy) { model.removeRoute(id) } }
                }
            }
            if (model.routes.size < 16) row { SecondaryButton("Add task route", enabled = !model.busy) { model.addRoute() } }
            row { SecondaryButton(if (advanced) "Hide installation options" else "Advanced installation", enabled = !model.busy) { advanced = !advanced } }
            if (advanced) {
                runtimeText(model, "Hermes source folder", null, "root", "Find the installed Hermes automatically")
                runtimeText(model, "Python executable", null, "python", "Use Hermes's installed Python")
            }
            model.installation?.let { installation ->
                row {
                    if (installation["available"]?.jsonPrimitive?.booleanOrNull == true) {
                        Text("Saved Hermes installation found", style = Type.caption, color = Palette.secondary)
                    } else ErrorRow(installation["error"]?.jsonPrimitive?.contentOrNull ?: "Hermes installation is unavailable.")
                }
                installation["profileSummary"]?.jsonPrimitive?.contentOrNull?.let { summary ->
                    row { Text(summary, style = Type.caption, color = Palette.secondary) }
                }
            }
            model.installationError?.let { row { ErrorRow(it) } }
            row { Text("Installation checks do not connect to SSH, Docker or a cloud environment.", style = Type.caption, color = Palette.secondary) }
            if (!model.validNumbers) row { ErrorRow("Enter whole numbers for the port and delegation limits, or leave them blank to use Hermes settings.") }
            row { PrimaryButton(if (model.busy) "Applying…" else "Apply", enabled = online && !model.loading && !model.busy && !model.checking && model.dirty && model.validNumbers) { model.apply() } }
            row { SecondaryButton("Check saved installation", enabled = online && !model.busy && !model.checking) { model.checkInstallation() } }
            row { SecondaryButton("Check saved environment (may start sandbox)", enabled = online && model.savedEnabled && !model.loading && !model.busy && !model.checking) { model.checkEnvironment() } }
        }
        model.message?.let { row { Text(it, style = Type.caption, color = Palette.secondary) } }
        model.error?.let { row { ErrorRow(it) } }
        if (!online) row { Text("Connect to this computer to load or apply Hermes settings.", style = Type.caption, color = Palette.secondary) }
        row { SecondaryButton("Reload saved settings", enabled = online && !model.loading && !model.busy && !model.checking) { model.reload() } }
    }
}

private fun CardRows.runtimeText(model: HermesRuntimeModel, label: String, section: String?, key: String,
    placeholder: String) {
    row { Field(label) {
        FieldBox(model.text(section, key), { if (!model.busy) model.edit(section, key, it) }, placeholder, capitalize = false)
    } }
}

private fun CardRows.runtimeChoice(model: HermesRuntimeModel, label: String, section: String?, key: String,
    options: List<Pair<String, String>>) {
    row { Field(label) {
        val saved = model.text(section, key)
        val selected = if (section == null && key == "memoryProvider" && saved.isEmpty()) "native" else saved
        ChoicePicker(selected, options, { model.edit(section, key, it) }, label = label, enabled = !model.busy)
    } }
}

/** Keeps network work and response handling out of the composable; business validation stays in the host. */
private class HermesRuntimeModel(private val botId: String, private val store: AppStore, private val generation: Int,
    private val scope: CoroutineScope) {
    var config by mutableStateOf<JsonObject?>(null); private set
    var installation by mutableStateOf<JsonObject?>(null); private set
    var loading by mutableStateOf(false); private set
    var busy by mutableStateOf(false); private set
    var checking by mutableStateOf(false); private set
    var error by mutableStateOf<String?>(null); private set
    var installationError by mutableStateOf<String?>(null); private set
    var message by mutableStateOf<String?>(null); private set
    private var baseline: JsonObject? = null
    private var revision: JsonElement = JsonNull
    private val numericFields = listOf("terminal" to "ssh_port", "delegation" to "max_iterations",
        "delegation" to "max_concurrent_children", "delegation" to "child_timeout_seconds")
    val dirty get() = config != baseline
    val enabled get() = config?.get("enabled")?.jsonPrimitive?.booleanOrNull == true
    val savedEnabled get() = baseline?.get("enabled")?.jsonPrimitive?.booleanOrNull == true
    val validNumbers get() = numericFields.all { (section, key) -> text(section, key).let { it.isBlank() || it.toIntOrNull() != null } }
    private fun active() = store.state.value.generation == generation
    private fun objectAt(section: String?) = if (section == null) config else config?.get(section) as? JsonObject
    fun text(section: String?, key: String) = (objectAt(section)?.get(key) as? JsonPrimitive)?.contentOrNull.orEmpty()
    fun enable(enabled: Boolean) { update(null, "enabled", JsonPrimitive(enabled)) }
    fun edit(section: String?, key: String, value: String) { update(section, key, JsonPrimitive(value)) }

    val routes get() = (objectAt("delegation")?.get("routes") as? JsonArray)?.toList().orEmpty()
    fun addRoute() {
        if (routes.size >= 16) return
        update("delegation", "routes", JsonArray(routes + buildJsonObject {
            put("id", java.util.UUID.randomUUID().toString()); put("label", ""); put("provider", ""); put("model", "")
        }))
    }
    fun editRoute(id: String, field: String, value: String) {
        if (field !in setOf("label", "provider", "model")) return
        update("delegation", "routes", JsonArray(routes.map { element ->
            val route = element as? JsonObject
            if (route != null && route["id"]?.jsonPrimitive?.contentOrNull == id) JsonObject(route + (field to JsonPrimitive(value))) else element
        }))
    }
    fun removeRoute(id: String) {
        update("delegation", "routes", JsonArray(routes.filterNot { (it as? JsonObject)?.get("id")?.jsonPrimitive?.contentOrNull == id }))
    }

    private fun update(section: String?, key: String, value: JsonElement) {
        if (busy || !active()) return
        val current = config ?: return
        config = if (section == null) JsonObject(current + (key to value)) else {
            val fields = current[section] as? JsonObject ?: JsonObject(emptyMap())
            JsonObject(current + (section to JsonObject(fields + (key to value))))
        }
        message = null
    }

    private suspend fun call(method: String, values: JsonObject = buildJsonObject {}) =
        store.hostQuery(method, JsonObject(values + ("botId" to JsonPrimitive(botId))), timeoutMillis = if (method == "hermesEnvironmentHealth") 65_000L else 20_000L, expectedGeneration = generation).jsonObject

    private fun accept(response: JsonObject) {
        val saved = response["config"] as? JsonObject ?: kotlin.error("The host returned incomplete Hermes settings.")
        config = saved
        baseline = saved
        revision = response["revision"] ?: JsonNull
    }

    suspend fun load() {
        if (loading || busy || !active()) return
        loading = true; error = null; message = null
        try {
            val response = call("hermesRuntime")
            if (active()) accept(response)
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { if (active()) error = describe(failure, "Couldn't load Hermes settings.") }
        finally { loading = false }
    }

    fun reload() { scope.launch { load() } }

    fun checkInstallation() {
        if (checking || busy || loading || !active()) return
        checking = true; installationError = null
        val expected = revision
        scope.launch {
            try {
                val response = call("hermesRuntimeStatus")
                if (active()) {
                    check((response["revision"] ?: JsonNull) == expected && revision == expected) { "Saved Hermes settings changed. Reload them before checking the installation." }
                    installation = response["installation"] as? JsonObject
                        ?: kotlin.error("The host returned no installation status.")
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { if (active()) installationError = describe(failure, "Couldn't check the saved installation.") }
            finally { checking = false }
        }
    }

    fun checkEnvironment() {
        if (checking || busy || loading || !active() || !savedEnabled) return
        checking = true; error = null
        val expected = revision
        scope.launch {
            try {
                val response = call("hermesEnvironmentHealth", buildJsonObject { put("ifRevision", expected) })
                if (active()) {
                    check((response["revision"] ?: JsonNull) == expected && revision == expected) { "Saved Hermes settings changed. Reload them before checking the environment." }
                    message = listOf("backend", "status", "detail").mapNotNull { response[it]?.jsonPrimitive?.contentOrNull }.joinToString(": ")
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { if (active()) error = describe(failure, "Couldn't check the saved environment.") }
            finally { if (active()) checking = false }
        }
    }

    private fun payload(): JsonObject {
        var result = config ?: kotlin.error("Load Hermes settings before applying.")
        for ((section, key) in numericFields) {
            val text = text(section, key).trim()
            val value = if (text.isEmpty()) JsonNull else JsonPrimitive(text.toIntOrNull() ?: kotlin.error("Enter a whole number for $key."))
            val fields = result[section] as? JsonObject ?: JsonObject(emptyMap())
            result = JsonObject(result + (section to JsonObject(fields + (key to value))))
        }
        return result
    }

    fun apply() {
        if (busy || loading || checking || !dirty || !validNumbers || !active()) return
        val requested = payload()
        val expected = revision
        busy = true; error = null; message = null
        scope.launch {
            try {
                val response = call("setHermesRuntime", buildJsonObject { put("config", requested); put("ifRevision", expected) })
                if (active()) {
                    accept(response)
                    installation = null
                    message = "Saved. Changes take effect after the current turn."
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { if (active()) error = describe(failure, "Couldn't apply Hermes settings.") }
            finally { busy = false }
        }
    }

    private fun describe(failure: Exception, fallback: String): String {
        val detail = failure.message.orEmpty()
        return if (listOf("unknown method", "method not found", "-32601").any { detail.contains(it, ignoreCase = true) }) {
            "This computer doesn't support Hermes runtime settings. Update Codync Dev on this computer and reopen Settings."
        } else detail.ifBlank { fallback }
    }
}
