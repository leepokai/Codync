package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.InstalledConnector
import com.codync.android.core.MarketConnector
import com.codync.android.design.*
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/**
 * Adds a registry connector (iOS `InstallConnectorSheet`): where it runs, its keys, then its sign-in. `addsAtOnce`: opened
 * by an Add, so a connector with nothing to fill in installs at once; opened to look at it, nothing installs until Add.
 */
@Composable internal fun InstallConnectorSheet(item: MarketConnector, state: AppState, store: AppStore, requestId: String? = null,
    addsAtOnce: Boolean = true, done: () -> Unit = {}) {
    val dismiss = LocalDismissModal.current
    val scope = rememberCoroutineScope()
    var optionId by remember(item.name) { mutableStateOf(item.options.firstOrNull()?.id.orEmpty()) }
    var values by remember(item.name) { mutableStateOf(emptyMap<String, String>()) }
    var installed by remember(item.name) { mutableStateOf<InstalledConnector?>(null) }
    var saving by remember { mutableStateOf(false) }
    var signingIn by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val option = item.options.firstOrNull { it.id == optionId } ?: item.options.firstOrNull()
    fun install() {
        val choice = option ?: return
        if (saving) return
        saving = true; error = null
        scope.launch {
            try {
                // The host keeps the connector a try saved unless new values or another way to run it come in.
                val connector = store.installConnector(item, choice, values).also { installed = it; values = emptyMap() }
                if (connector.needsSignIn) {
                    // The store finishes a chat's request itself once the sign-in comes back.
                    signingIn = true
                    if (!store.signInConnector(connector.id, requestId)) throw IOException("Sign-in wasn't confirmed. Try Add again to sign in.")
                } else if (requestId != null) store.finishConnectionRequest(requestId, connectorId = connector.id)
                if (requestId == null) store.verifyConnector(connector.id)
                done(); dismiss()
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Couldn't add this connector." }
            finally { saving = false; signingIn = false }
        }
    }
    // Opened by an Add with nothing to fill in: it adds itself.
    LaunchedEffect(item.name) { if (addsAtOnce && item.options.size == 1 && item.options[0].inputs.isEmpty()) install() }
    // After a try that failed, Add with nothing typed tries the connector it saved again.
    val ready = option?.complete(values) == true || (installed != null && values.values.all { it.isEmpty() })
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Add connector") {
            if (saving) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else IconButton("Add", "plus", enabled = ready) { install() }
        }
        CardForm(Modifier.weight(1f)) {
            CardSection {
                row {
                    Column(Modifier.padding(vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(item.title, style = Type.title3.semibold, color = Palette.text)
                        item.description?.let { Text(it, style = Type.body, color = Palette.secondary) }
                        item.website?.let { WebLink(it, it, Type.footnote) }
                    }
                }
            }
            if (item.options.size > 1) CardSection("Runs") {
                row {
                    val hosted = tr("Hosted by {name}")
                    val onComputer = tr("On {name} ({kind})")
                    ChoiceList(optionId, item.options.map {
                        Triple(it.id, if (it.kind == "remote") hosted.replace("{name}", item.title)
                            else onComputer.replace("{name}", state.hostName).replace("{kind}", it.kind), null)
                    },
                        { optionId = it; values = emptyMap() })
                }
            }
            if (option != null) CardSection("Setup", tr("Keys are saved on {name} only.").replace("{name}", state.hostName)) {
                if (option.inputs.isEmpty()) row {
                    Text(if (option.kind == "remote") tr("No keys needed here. If {name} wants you to sign in, its sign-in page opens next.").replace("{name}", item.title) else tr("No setup needed."),
                        style = Type.body, color = Palette.secondary)
                }
                for (input in option.inputs) row {
                    NotedInput(values[input.name].orEmpty(), { values = values + (input.name to it) },
                        if (input.secret) input.name else input.defaultValue ?: input.placeholder ?: input.name,
                        input.description?.let { it + if (input.required) "" else tr(" (optional)") }, secret = input.secret)
                }
            }
            if (signingIn) CardSection { row { Waiting(tr("Finish signing in to {name} in the browser…").replace("{name}", item.title)) } }
            error?.let { CardSection { row { ErrorRow(it) } } }
        }
    }
}

/** A spinner with what it waits for. */
@Composable internal fun Waiting(text: String) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Spinner()
        Text(text, style = Type.body, color = Palette.secondary)
    }
}

private enum class CustomMode { Command, Url, Config }

/** Any MCP server: a command, a URL, or a pasted config (iOS `CustomConnectorSheet`). */
@Composable internal fun CustomConnectorSheet(state: AppState, store: AppStore) {
    val dismiss = LocalDismissModal.current
    val scope = rememberCoroutineScope()
    var mode by remember { mutableStateOf(CustomMode.Command) }
    var name by remember { mutableStateOf("") }
    var target by remember { mutableStateOf("") }
    var envText by remember { mutableStateOf("") }
    var config by remember { mutableStateOf("") }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val ready = if (mode == CustomMode.Config) config.isNotBlank() else name.isNotBlank() && target.isNotBlank()
    fun save() {
        saving = true; error = null
        val remote = mode == CustomMode.Url
        val pairs = envText.lines().mapNotNull { line ->
            val parts = line.split(if (remote) ':' else '=', limit = 2).map(String::trim)
            if (parts.size == 2 && parts[0].isNotEmpty()) parts[0] to parts[1] else null
        }.toMap()
        scope.launch {
            try {
                val added = if (mode == CustomMode.Config) store.importConnectors(config)
                    else listOf(store.addConnector(name.trim(), target.trim().takeUnless { remote }, target.trim().takeIf { remote },
                        if (remote) emptyMap() else pairs, if (remote) pairs else emptyMap()))
                for (connector in added.filter { it.needsSignIn }) {
                    if (!store.signInConnector(connector.id)) throw IOException("${connector.name} was added, but its sign-in wasn't confirmed. Sign in from Installed.")
                }
                dismiss()
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Couldn't add this connector." }
            finally { saving = false }
        }
    }
    val host = state.hostName
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Custom connector") {
            if (saving) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else IconButton("Add", "plus", enabled = ready) { save() }
        }
        CardForm(Modifier.weight(1f)) {
            CardSection(footer = when (mode) {
                CustomMode.Command -> tr("Runs on {name} in the bot's project folder. Quotes work like in a shell.").replace("{name}", host)
                CustomMode.Url -> "A remote MCP server, streamable HTTP or SSE."
                CustomMode.Config -> tr("Paste the MCP config from a README or another app (Claude, Cursor, VS Code). Every server in it is added. Saved on {name} only.").replace("{name}", host)
            }) {
                row { SegmentedChoice(mode, listOf(CustomMode.Command to "Command", CustomMode.Url to "URL", CustomMode.Config to "Config"), { mode = it }) }
                if (mode == CustomMode.Config) row {
                    PlainInput(config, { config = it }, "{ \"mcpServers\": { … } }", style = Type.callout.monospaced, minLines = 6, maxLines = 16)
                } else {
                    row { PlainInput(name, { name = it }, "Name") }
                    row { PlainInput(target, { target = it }, if (mode == CustomMode.Url) "https://example.com/mcp" else "npx -y @scope/server", style = Type.callout.monospaced) }
                }
            }
            if (mode == CustomMode.Url) CardSection("Headers", tr("Optional, one Name: value per line. Leave empty if the service has you sign in. Saved on {name} only.").replace("{name}", host)) {
                row { PlainInput(envText, { envText = it }, "Authorization: Bearer …", style = Type.callout.monospaced, minLines = 2, maxLines = 6) }
            } else if (mode == CustomMode.Command) CardSection("Environment", tr("One KEY=value per line. Saved on {name} only.").replace("{name}", host)) {
                row { PlainInput(envText, { envText = it }, "API_KEY=…", style = Type.callout.monospaced, minLines = 2, maxLines = 6) }
            }
            error?.let { CardSection { row { ErrorRow(it) } } }
        }
    }
}

/** Write a skill a bot can follow (iOS `NewSkillSheet`). */
@Composable internal fun NewSkillSheet(store: AppStore) {
    val dismiss = LocalDismissModal.current
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf("") }
    var summary by remember { mutableStateOf("") }
    var instructions by remember { mutableStateOf("") }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("New skill") {
            if (saving) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else IconButton("Save", "checkmark", enabled = name.isNotBlank() && instructions.isNotBlank()) {
                saving = true; error = null
                scope.launch {
                    try { store.addSkill(name, summary, instructions); dismiss() }
                    catch (cancelled: CancellationException) { throw cancelled }
                    catch (failure: Exception) { error = failure.message ?: "Couldn't save the skill." }
                    finally { saving = false }
                }
            }
        }
        CardForm(Modifier.weight(1f)) {
            CardSection(footer = "The bot sees the name and when to use it, and reads the instructions only when a task fits.") {
                row { PlainInput(name, { name = it }, "Name") }
                row { PlainInput(summary, { summary = it }, "When to use it", minLines = 2, maxLines = 4) }
            }
            CardSection("Instructions") { row { PlainInput(instructions, { instructions = it }, "Step by step, in plain words…", minLines = 6, maxLines = 20) } }
            error?.let { CardSection { row { ErrorRow(it) } } }
        }
    }
}

/** The skill a details sheet shows: an installed one, or one from the catalog (`source`). */
internal data class SkillRef(val id: String, val name: String, val description: String, val source: String? = null, val installed: Boolean = true)

/** A skill's details (iOS `SkillSheet`): what it tells the bot, its `SKILL.md`, with Add while it isn't installed. */
@Composable internal fun SkillSheet(skill: SkillRef, state: AppState, store: AppStore) {
    val scope = rememberCoroutineScope()
    var markdown by remember(skill.id) { mutableStateOf<String?>(null) }
    var error by remember(skill.id) { mutableStateOf<String?>(null) }
    var adding by remember { mutableStateOf(false) }
    val added = skill.source == null || skill.installed || state.plugins?.skills?.any { it.id == skill.source.lowercase() } == true
    LaunchedEffect(skill.id) {
        try { markdown = store.skillDetails(skill.id, skill.source) }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't load this skill." }
    }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(skill.name) {
            when {
                added -> Box(Modifier.size(32.dp), contentAlignment = Alignment.Center) { Symbol("checkmark", size = 16.dp, tint = Palette.secondary, contentDescription = "Added") }
                adding -> Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
                else -> IconButton("Add", "plus") {
                    adding = true; error = null
                    scope.launch {
                        try { store.installSkill(requireNotNull(skill.source)) }
                        catch (cancelled: CancellationException) { throw cancelled }
                        catch (failure: Exception) { error = failure.message ?: "Couldn't add this skill." }
                        finally { adding = false }
                    }
                }
            }
        }
        CardForm(Modifier.weight(1f)) {
            if (skill.description.isNotEmpty()) CardSection { row { Text(skill.description, style = Type.subheadline, color = Palette.secondary) } }
            CardSection("Instructions", if (added) "Bots read these before using the skill." else "Add it to let bots use these instructions.") {
                row {
                    val text = markdown
                    when {
                        text == null -> if (error == null) Spinner()
                        text.isEmpty() -> Text("No instructions.", style = Type.body, color = Palette.secondary)
                        else -> MarkdownText(text)
                    }
                }
            }
            error?.let { CardSection { row { Text(it, style = Type.footnote, color = Palette.danger) } } }
        }
    }
}
