package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.BotDraft
import com.codync.android.core.LinkState
import com.codync.android.core.ScreenState
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive

/**
 * Which settings a form shows (iOS `FormPart`): all of them (the editor), what changes often (instructions and the agent:
 * the details sheet's Details page) or the rest (its Settings page).
 */
internal enum class FormPart { All, Often, Rarely }

/** Bot settings, in sections in the Settings style (`CardSection`), with the pickers they open. */
@Composable internal fun BotSettingsForm(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit, part: FormPart,
    modifier: Modifier = Modifier) {
    var folders by remember { mutableStateOf(false) }
    var viewingSkill by remember { mutableStateOf<SkillRef?>(null) }
    var addingFallback by remember { mutableStateOf(false) }
    val online = state.actualConnection is LinkState.Ready
    val bot = state.bots.firstOrNull { it.id == draft.id }
    LaunchedEffect(online) { if (online) store.refreshPlugins() }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(30.dp)) {
        when (part) {
            FormPart.All -> {
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Box(Modifier.clip(RoundedRectangle(24.dp)).background(Palette.bubbleAgent).padding(18.dp)) {
                        CharacterAvatar(draft.avatarShape, draft.avatarColor, 96.dp)
                    }
                    AvatarPicker(draft.avatarShape, draft.avatarColor, { change(draft.copy(avatarShape = it)) }, { change(draft.copy(avatarColor = it)) })
                    draft.id?.let { CustomIconEditor("bot:$it", state, store) }
                }
                CardSection("Profile") { nameRow(draft, change); instructionsRow(draft, change); createdByRow(state, bot) }
                CardSection("Agent") {
                    fallbackBanner(state, store, bot); computerRow(state, online)
                    agentRows(state, store, draft, change, bot) { addingFallback = true }; placeRows(draft, change) { folders = true }
                }
                CardSection("Activity") {
                    notifyRow(draft, change); computerUseRow(state, draft, change); manageTeamRow(state, draft, change); reachRows(state, draft, change)
                }
                Plugins(state, draft, change) { viewingSkill = it }
                draft.id?.let { MemoryCard(it, store) }
                state.error?.let { Text(it, style = Type.footnote, color = Palette.danger) }
            }
            FormPart.Often -> {
                CardSection("Instructions") { instructionsRow(draft, change) }
                CardSection("Agent") {
                    fallbackBanner(state, store, bot)
                    agentRows(state, store, draft, change, bot) { addingFallback = true }
                    if (!online) row { OfflineNote() }
                }
            }
            FormPart.Rarely -> {
                CardSection("Profile") {
                    row { AvatarPicker(draft.avatarShape, draft.avatarColor, { change(draft.copy(avatarShape = it)) }, { change(draft.copy(avatarColor = it)) }) }
                    draft.id?.let { id -> row { CustomIconEditor("bot:$id", state, store) } }
                    nameRow(draft, change); createdByRow(state, bot)
                }
                // The agent and its model are on the Details page; what's left is where it works.
                CardSection("Workspace") { computerRow(state, online); placeRows(draft, change) { folders = true } }
                CardSection("Activity") { notifyRow(draft, change); manageTeamRow(state, draft, change); reachRows(state, draft, change) }
                Plugins(state, draft, change) { viewingSkill = it }
            }
        }
    }
    CodyncSheet(folders, { folders = false }) {
        FolderPicker(draft.cwd.ifEmpty { state.hello?.get("home")?.jsonPrimitive?.contentOrNull }, online, state.hello?.get("name")?.jsonPrimitive?.contentOrNull ?: "Computer", store::listDirs, store::selectWorkspace) {
            change(draft.copy(cwd = it)); folders = false
        }
    }
    CodyncSheet(viewingSkill, { viewingSkill = null }) { SkillSheet(it, state, store) }
    CodyncSheet(addingFallback, { addingFallback = false }) {
        AddFallbackSheet(state, store, draft.backend, draft.fallbacks.orEmpty()) { change(draft.copy(fallbacks = draft.fallbacks.orEmpty() + it)); addingFallback = false }
    }
}

@Composable private fun OfflineNote() = Text(tr("Reconnect to this computer to save settings."), style = Type.caption, color = Palette.warning)

// A bot left unnamed at creation is named from its first conversations; rename it any time after.
private fun CardRows.nameRow(draft: BotDraft, change: (BotDraft) -> Unit) =
    row { Field("Name") { FieldBox(draft.name, { change(draft.copy(name = it)) }, "Name") } }

private fun CardRows.instructionsRow(draft: BotDraft, change: (BotDraft) -> Unit) = row {
    Field("Standing instructions", "Rules that always apply. Put task-specific requests in the chat instead.") {
        FieldBox(draft.description, { change(draft.copy(description = it)) }, "e.g. Reviews PRs. Never pushes without asking.", 3, 8)
    }
}

private fun CardRows.createdByRow(state: AppState, bot: Bot?) {
    val creator = bot?.createdBy ?: return
    row { ValueRow("Created by", "${state.bots.firstOrNull { it.id == creator }?.name ?: "A bot"} · ${RelativeTime.calendarDay(bot.createdAt)}") }
}

/** While the bot runs on a fallback: which, and Switch back. */
private fun CardRows.fallbackBanner(state: AppState, store: AppStore, bot: Bot?) {
    val step = bot?.agentStep ?: return
    row { FallbackBanner(state, store, bot, step) }
}

private fun CardRows.computerRow(state: AppState, online: Boolean) = row {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        ValueRow("Computer", state.computers.firstOrNull()?.name ?: "Your computer")
        if (!online) OfflineNote()
    }
}

/** The agent, its model and the fallbacks that carry on for it. */
private fun CardRows.agentRows(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit, bot: Bot?, addFallback: () -> Unit) {
    row {
        AgentModelPicker(state, store::agentModels, draft.backend, draft.model, draft.modelOptions, draft.fastMode ?: false, bot,
            { model, options -> change(draft.copy(model = model, modelOptions = options)) }, { change(draft.copy(fastMode = it)) },
            agents = agentChoices(state) + ("custom" to "Custom command"),
            chooseAgent = { if (it != draft.backend) change(draft.withAgent(it)) }) {
                bot?.restOf(draft.backend)?.let { Text(it.line(RelativeTime.clock(it.until)), style = Type.caption, color = Palette.secondary) }
                backends(state).firstOrNull { it.text("id") == draft.backend && !it.available }?.text("installHint")?.takeIf(String::isNotBlank)?.let {
                    Text(it, style = Type.caption, color = Palette.warning)
                }
                if (draft.backend == "custom") FieldBox(draft.command.orEmpty(), { change(draft.copy(command = it.ifEmpty { null })) },
                    "ACP command, e.g. my-agent --acp", style = Type.callout.monospaced, capitalize = false)
            }
    }
    // Only from a host that has fallbacks (its bots say).
    val list = draft.fallbacks ?: return
    row { FallbackList(state, draft.backend, list, bot, { change(draft.copy(fallbacks = it)) }, addFallback) }
    row {
        SwitchRow("Let the bot change its fallbacks", "It can set this list itself, for example when you ask it to.", draft.selfFallbacks != false,
            { change(draft.copy(selfFallbacks = it)) })
    }
}

/** Where the bot works and what it may do there without asking. */
private fun CardRows.placeRows(draft: BotDraft, change: (BotDraft) -> Unit, chooseFolder: () -> Unit) {
    row {
        ValueRow("Workspace", detail = if (draft.cwd.isEmpty()) "This bot has its own space for files. It can work in other folders when you ask."
            else "This project is the default starting folder. The bot can work elsewhere when you ask.") {
            DropdownMenu({ listOf(
                MenuItem("Personal workspace", selected = draft.cwd.isEmpty()) { change(draft.copy(cwd = "")) },
                MenuItem("Choose project folder…", selected = draft.cwd.isNotEmpty(), action = chooseFolder),
            ) }, label = "Workspace") {
                Row(Modifier.outline().pill(androidx.compose.ui.graphics.Color.Transparent), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(if (draft.cwd.isEmpty()) tr("Personal") else draft.cwd.trimEnd('/').substringAfterLast('/').ifEmpty { "/" },
                        style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Symbol("chevron.down", size = 13.dp, tint = Palette.secondary)
                }
            }
        }
    }
    row {
        ValueRow("Permissions", detail = if (draft.permission == "auto")
            "Tool requests are approved automatically. Your agent's own settings (like Claude Code's permission rules) still apply."
            else "You'll get an approval card and a notification whenever the agent asks.") {
            ChoicePicker(draft.permission, listOf("ask" to "Ask me", "auto" to "Approve automatically"), { change(draft.copy(permission = it)) },
                label = "Permissions")
        }
    }
}

private fun CardRows.notifyRow(draft: BotDraft, change: (BotDraft) -> Unit) =
    row { SwitchRow("Notifications", "Get notified when this bot finishes or needs you", draft.notify != false, { change(draft.copy(notify = it)) }) }

private fun CardRows.computerUseRow(state: AppState, draft: BotDraft, change: (BotDraft) -> Unit) {
    val screen = state.screen ?: return
    row { SwitchRow("Use the computer", computerUseDetail(screen), draft.computer, { change(draft.copy(computer = it)) }) }
}

/** What turning on "Use the computer" does on this computer, or what it needs first. */
private fun computerUseDetail(screen: ScreenState): String = when {
    !screen.controlReady -> "Let this bot use apps. On that computer, open Codync → Settings → Computer access to set up permissions first."
    screen.enabled -> "Let this bot see the screen and use the mouse and keyboard. You can watch and take over from your phone."
    screen.computerUse -> "Let this bot see the screen and use the mouse and keyboard."
    else -> "Let this bot see the screen and use the mouse and keyboard. On that computer, open Codync → Settings → Computer access to set up permissions first."
}

// Only from a host that has it (its bots say).
private fun CardRows.manageTeamRow(state: AppState, draft: BotDraft, change: (BotDraft) -> Unit) {
    if (state.bots.none { it.manageTeam != null }) return
    row {
        SwitchRow("Manage the team", "Let this bot create bots and change any bot's setup, its own included. The bots it creates can too.",
            draft.manageTeam, { change(draft.copy(manageTeam = it)) })
    }
}

// Who else reaches this bot: shown only where it applies (in a network, cloud assistants on) and the host has the switch.
private fun CardRows.reachRows(state: AppState, draft: BotDraft, change: (BotDraft) -> Unit) {
    val network = state.network ?: return
    draft.offeredToNetwork?.takeIf { network.joined }?.let { on ->
        row { SwitchRow("Offered to the network", "Bots on your other computers can see and message this bot.", on, { change(draft.copy(offeredToNetwork = it)) }) }
    }
    draft.reachableByAssistants?.takeIf { network.assistants }?.let { on ->
        row {
            SwitchRow("Reachable by cloud assistants", "ChatGPT or Claude in your Codync account can see and message this bot.", on,
                { change(draft.copy(reachableByAssistants = it)) })
        }
    }
}

@Composable private fun Plugins(state: AppState, draft: BotDraft, change: (BotDraft) -> Unit, viewSkill: (SkillRef) -> Unit) {
    val plugins = state.plugins
    PluginToggles("Connectors", "No connectors yet. Add GitHub, Linear, Notion and more from Extensions.",
        plugins?.connectors.orEmpty().map { Triple(it.id, it.name, it.command ?: it.url.orEmpty()) },
        draft.connectors ?: plugins?.connectors.orEmpty().map { it.id }, { change(draft.copy(connectors = it)) })
    PluginToggles("Skills", "No skills yet. Get some from Extensions, or write your own.",
        plugins?.skills.orEmpty().map { Triple(it.id, it.name, it.description) }, draft.skills.orEmpty(), { change(draft.copy(skills = it)) }) { id ->
        plugins?.skills?.firstOrNull { it.id == id }?.let { viewSkill(SkillRef(it.id, it.name, it.description)) }
    }
}
