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
import androidx.compose.runtime.mutableIntStateOf
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
import com.codync.android.core.GitState
import com.codync.android.core.gitState
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.testTagsAsResourceId
import com.codync.android.core.LinkState
import com.codync.android.core.Login
import com.codync.android.core.LoginSeat
import com.codync.android.core.MAX_THREADS
import com.codync.android.core.login
import com.codync.android.core.restOf
import com.codync.android.core.ScreenState
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics

/**
 * Bot settings, in sections in the Settings style (`CardSection`), with the pickers they open: what it is told and what
 * runs it first. [inDetails]: on the details sheet, whose Computer page has the computer's switch.
 */
@Composable internal fun BotSettingsForm(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit,
    modifier: Modifier = Modifier, inDetails: Boolean = false, error: String? = state.error) {
    var folders by remember { mutableStateOf(false) }
    // The Avatar row unfolds the picker (iOS `pickingAvatar`).
    var pickingAvatar by remember { mutableStateOf(false) }
    var viewingSkill by remember { mutableStateOf<SkillRef?>(null) }
    // Retry model list reloads Permissions too: its modes come from the same list.
    var modelReload by remember { mutableIntStateOf(0) }
    val online = state.actualConnection is LinkState.Ready
    val bot = state.bots.firstOrNull { it.id == draft.id }
    LaunchedEffect(online) { if (online) store.refreshPlugins() }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(30.dp)) {
        CardSection("Instructions", fold = "bot.instructions") { instructionsRow(draft, change) }
        CardSection("Agent", fold = "bot.agent") {
            fallbackBanner(state, store, bot)
            agentRows(state, store, draft, change, bot, modelReload, reloadModels = { modelReload++ })
            compactionRow(bot, draft, change)
        }
        CardSection("Profile", fold = "bot.profile") {
            avatarRows(state, store, draft, change, pickingAvatar) { pickingAvatar = !pickingAvatar }
            nameRow(draft, change); createdByRow(state, bot)
        }
        CardSection("Workspace", fold = "bot.workspace") {
            computerRow(state, online); placeRows(state, store, draft, change, modelReload) { folders = true }
        }
        CardSection("Activity", fold = "bot.activity") {
            notifyRow(draft, change)
            if (!inDetails) computerUseRow(state, draft, change)
            manageTeamRow(state, draft, change); reachRows(state, draft, change)
        }
        Plugins(state, draft, change) { viewingSkill = it }
        // A save that didn't take, under the form (iOS `error`).
        error?.let { Text(tr(it), style = Type.footnote, color = Palette.danger) }
    }
    CodyncSheet(folders, { folders = false }) {
        // With projects, as on the iPhone: from the default project, else the computer's home.
        val start = if (state.botProjects) draft.projects?.firstOrNull() else draft.cwd.ifEmpty { null }
        FolderPicker(start ?: state.hello?.get("home")?.jsonPrimitive?.contentOrNull, online, state.computerName, store::listDirs, store::selectWorkspace) {
            change(if (state.botProjects) draft.withProject(it) else draft.copy(cwd = it)); folders = false
        }
    }
    CodyncSheet(viewingSkill, { viewingSkill = null }) { SkillSheet(it, state, store) }
}

@Composable private fun OfflineNote() = Text(tr("Reconnect to this computer to save settings."), style = Type.caption, color = Palette.warning)

// A bot left unnamed at creation is named from its first conversations; rename it any time after.
/** "Avatar" with the face as it is; tapping it unfolds the picker and the custom icon (iOS `BotEditorView` Profile). */
private fun CardRows.avatarRows(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit, picking: Boolean, toggle: () -> Unit) {
    row {
        val icon = draft.id?.let { state.icons["bot:$it"] }
        Box(Modifier.fillMaxWidth().plainButton(action = toggle).semantics { contentDescription = "Edit Bot avatar" }) {
            ValueRow("Avatar") {
                Box(Modifier.clip(RoundedRectangle(10.dp)).background(Palette.bubbleAgent).padding(5.dp)) {
                    CharacterAvatar(draft.avatarShape, draft.avatarColor, 28.dp, customImage = icon?.dataUrl, customTinted = icon?.tinted == true)
                }
            }
        }
    }
    if (picking) row {
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
            AvatarPicker(draft.avatarShape, draft.avatarColor, { change(draft.copy(avatarShape = it)) }, { change(draft.copy(avatarColor = it)) })
            draft.id?.let { CustomIconEditor("bot:$it", state, store) }
        }
    }
}

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
        ValueRow("Computer", state.hostName)
        if (!online) OfflineNote()
    }
}

/** The agent, its model, the fallbacks that carry on for it and Advisor mode's for its threads. */
private fun CardRows.agentRows(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit, bot: Bot?, modelReload: Int,
    reloadModels: () -> Unit) {
    row {
        AgentModelPicker(state, store::agentModels, draft.backend, draft.model, draft.modelOptions, draft.fastMode ?: false, bot,
            { model, options -> change(draft.copy(model = model, modelOptions = options)) }, { change(draft.copy(fastMode = it)) },
            agents = pickerSeats(agentBackends(state), draft.login) + LoginSeat(Login("custom"), tr("Custom command")),
            chooseAgent = { change(draft.withAgent(it)) },
            onReload = reloadModels, loginProfile = draft.loginProfile, store = store) {
                bot?.restOf(draft.login)?.let { Text(tr(it.line(RelativeTime.clock(it.until))), style = Type.caption, color = Palette.secondary) }
                backends(state).firstOrNull { it.text("id") == draft.backend && !it.available }?.text("installHint")?.takeIf(String::isNotBlank)?.let {
                    Text(it, style = Type.caption, color = Palette.warning)
                }
                if (draft.backend == "custom") FieldBox(draft.command.orEmpty(), { change(draft.copy(command = it.ifEmpty { null })) },
                    "ACP command, e.g. my-agent --acp", style = Type.callout.monospaced, capitalize = false)
            }
    }
    // Only from a host that has fallbacks (its bots say).
    val list = draft.fallbacks ?: return
    row { FallbackList(state, store, draft.login, list, bot, { change(draft.copy(fallbacks = it)) }) }
    row {
        SwitchRow("Let the bot change its fallbacks", "It can set this list itself, for example when you ask it to.", draft.selfFallbacks != false,
            { change(draft.copy(selfFallbacks = it)) })
    }
    advisorRow(state, store, draft, change, bot)
}

/** Where the bot works, how many of its threads work at once and what it may do there without asking. */
private fun CardRows.placeRows(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit, modelReload: Int,
    chooseFolder: () -> Unit) {
    if (state.botProjects) projectRows(state.bots.firstOrNull { it.id == draft.id }, draft, state.worktreeFill, change, chooseFolder) else workspaceRows(state, draft, change, chooseFolder)
    // Only from a host that has the box (its bots say).
    draft.maxThreads?.let { count ->
        row { ValueRow("Threads at once") { Stepper(count, 1..MAX_THREADS, { change(draft.copy(maxThreads = it)) }) } }
    }
    // Only where it matters (a project folder in a git repository: a bot's own folder keeps all its work in one
    // place) and the host has it.
    if (!state.botProjects) draft.threadWorktrees?.takeIf { state.bots.firstOrNull { it.id == draft.id }?.let { it.gitRepo && !it.managedWorkspace } == true }?.let { on ->
        row {
            SwitchRow("Worktree per thread", "Threads that work at once in this repository each get their own branch.", on,
                { change(draft.copy(threadWorktrees = it)) })
        }
    }
    row { ApprovalRow(state, store::agentModels, draft, change, modelReload) }
}

/** A host before projects: the one folder the bot works in, its own or a project. */
private fun CardRows.workspaceRows(state: AppState, draft: BotDraft, change: (BotDraft) -> Unit, chooseFolder: () -> Unit) {
    row {
        Column {
            ValueRow("Workspace", detail = if (draft.cwd.isEmpty()) "This bot has its own space for files. It can work in other folders when you ask."
                else "This project is the default starting folder. The bot can work elsewhere when you ask.") {
                DropdownMenu({ listOf(
                    MenuItem("Own folder", selected = draft.cwd.isEmpty()) { change(draft.copy(cwd = "")) },
                    MenuItem("Choose project folder…", selected = draft.cwd.isNotEmpty(), action = chooseFolder),
                ) }, label = "Workspace") {
                    Row(Modifier.outline().pill(androidx.compose.ui.graphics.Color.Transparent), verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(if (draft.cwd.isEmpty()) tr("Own folder") else draft.cwd.trimEnd('/').substringAfterLast('/').ifEmpty { "/" },
                            style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Symbol("chevron.down", size = 13.dp, tint = Palette.secondary)
                    }
                }
            }
            // The folder as the computer has it saved: a new bot has none yet, and an older host doesn't say.
            state.bots.firstOrNull { it.id == draft.id }?.let(::gitState)?.let { GitLine(it) }
        }
    }
}

/** Under the folder: whether it is a git repository, and the manual's page on what threads working at once do with and without git. */
@Composable private fun GitLine(git: GitState) {
    val uri = LocalUriHandler.current
    Row(Modifier.semantics { testTagsAsResourceId = true }.testTag("git-state"), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(tr(git.title), Modifier.weight(1f, fill = false), style = Type.caption, color = Palette.secondary)
        IconButton("About working folders and git", "info.circle") { uri.open("https://codync.dev/docs/working-folders", OutboundLink.Docs) }
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
        draft.connectors.orEmpty(), { change(draft.copy(connectors = it)) })
    PluginToggles("Skills", "No skills yet. Get some from Extensions, or write your own.",
        plugins?.skills.orEmpty().map { Triple(it.id, it.name, "") }, draft.skills.orEmpty(), { change(draft.copy(skills = it)) }) { id ->
        plugins?.skills?.firstOrNull { it.id == id }?.let { viewSkill(SkillRef(it.id, it.name, it.description)) }
    }
}
