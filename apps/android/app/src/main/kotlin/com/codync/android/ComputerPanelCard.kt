package com.codync.android

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.ComputerPermissions
import com.codync.android.design.*
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull

@Composable internal fun ComputerPanelCard(botId: String, state: AppState, store: AppStore,
    changed: (Boolean, ComputerPermissions) -> Unit) {
    val scope = rememberCoroutineScope()
    val model = remember(store, state.generation, botId) { ComputerPanelModel(botId, store, scope, state.generation) }
    var tab by remember(model) { mutableStateOf("status") }
    var path by remember(model) { mutableStateOf("") }
    var site by remember(model) { mutableStateOf("") }
    LaunchedEffect(model) { model.watch() }
    val snapshot = model.snapshot
    CardSection("Computer", "These permissions gate Codync tools on the real host, not arbitrary harness tools. Desktop access includes apps, mouse and keyboard. Revocation cancels pending work; effects already sent cannot be undone.") {
        row { SegmentedChoice(tab, listOf("status" to "Status", "files" to "Files", "activity" to "Activity"), { tab = it }) }
        row { SecondaryButton("Refresh computer", enabled = !model.loading) { model.refresh() } }
        if (model.loading && snapshot == null) row { Waiting("Loading computer…") }
        if (tab == "status" && snapshot != null) {
            row { Text("Workspace: ${snapshot.workspace}", style = Type.caption, color = Palette.secondary) }
            healthRows(store, state.generation)
            row { Text(if (snapshot.screen["userControl"]?.jsonPrimitive?.booleanOrNull == true) "You control the screen" else "Host controls the screen", style = Type.body, color = Palette.text) }
            row { SwitchRow("Use the computer", null, snapshot.enabled, {
                if (!model.busy) model.permissions(it, snapshot.permissions) { changed(it, snapshot.permissions) }
            }, enabled = !model.busy) }
            for ((id, title) in listOf("browser" to "Desktop and browser", "files" to "Workspace files", "shell" to "Shell commands")) row {
                val on = when (id) { "browser" -> snapshot.permissions.browser; "files" -> snapshot.permissions.files; else -> snapshot.permissions.shell }
                SwitchRow(title, "Enable computer access first. Shell commands run from desktop/TUI or agent tools.", on, { enabled ->
                    if (!model.busy && snapshot.enabled) {
                        val permissions = when (id) { "browser" -> snapshot.permissions.copy(browser = enabled); "files" -> snapshot.permissions.copy(files = enabled); else -> snapshot.permissions.copy(shell = enabled) }
                        model.permissions(snapshot.enabled, permissions) { changed(snapshot.enabled, permissions) }
                    }
                }, enabled = !model.busy && snapshot.enabled)
            }
            row { Text("Pause Codync tools and stop current bot work before entering a password or code yourself on the real host. Verify the website. Never paste secrets into chat. Protected browser autofill requires the local Windows owner session; finishing here closes that browser. Independent harness tools are not isolated.", style = Type.caption, color = Palette.secondary) }
            row { Text("Research: ${serviceLabel(snapshot.services["research"]?.jsonPrimitive?.contentOrNull)} · Slack: ${serviceLabel(snapshot.services["slack"]?.jsonPrimitive?.contentOrNull)}. Research keys are configured in Windows Settings; Slack setup is local to the host.", style = Type.caption, color = Palette.secondary) }
            val protected = snapshot.screen["protectedEntry"] as? JsonObject
            if (protected != null) {
                row { Text("Manual entry active: ${protected["site"]?.jsonPrimitive?.contentOrNull.orEmpty()}", style = Type.body, color = Palette.text) }
                row { SecondaryButton("Finish manual entry", enabled = !model.busy) { model.protectedEntry(null, protected["id"]?.jsonPrimitive?.contentOrNull) } }
            } else {
                row { PlainInput(site, { site = it }, "https://example.com (website, never the secret)") }
                row { SecondaryButton("Pause for manual entry", enabled = !model.busy && snapshot.enabled && snapshot.permissions.browser && site.isNotBlank()) { model.protectedEntry(site) } }
            }
        }
        if (tab == "files") {
            val allowed = snapshot?.enabled == true && snapshot.permissions.files
            if (!allowed) row { Text("Enable computer access and Workspace files in Status.", style = Type.body, color = Palette.secondary) }
            row { PlainInput(path, { path = it }, "Path relative to workspace") }
            row { SecondaryButton("Open folder", enabled = allowed) { model.open(path) } }
            row { Text("Folder: ${model.path.ifEmpty { "/" }}", style = Type.caption, color = Palette.secondary) }
            for (file in model.files) row {
                SecondaryButton((if (file.directory) "Folder: " else "") + file.name + if (file.link) " (link)" else "", enabled = allowed && !file.link) {
                    val next = listOf(model.path, file.name).filter { it.isNotEmpty() }.joinToString("/")
                    if (file.directory) path = next
                    model.open(next, file.directory)
                }
            }
            if (model.files.isEmpty()) row { Text("Open a folder to list its files. Empty folders show no entries.", style = Type.caption, color = Palette.secondary) }
            if (model.truncated) row { Text("Showing at most 500 entries. Open a more specific folder.", style = Type.caption, color = Palette.secondary) }
            model.text?.let { row { SelectionContainer { Text(it, style = Type.callout.monospaced, color = Palette.text) } } }
        }
        if (tab == "activity") {
            if (snapshot?.activity.isNullOrEmpty()) row { Text("No computer activity recorded.", style = Type.body, color = Palette.secondary) }
            for (item in snapshot?.activity.orEmpty()) row { Text("${item.action} · ${item.result} · ${java.util.Date(item.createdAt)}", style = Type.caption, color = Palette.secondary) }
        }
        model.error?.let { row { ErrorRow(it) } }
    }
}
private fun serviceLabel(value: String?) = when (value) {
    "configured" -> "configured"
    "notConfigured" -> "not configured"
    else -> "unavailable; unlock host credentials to check"
}
