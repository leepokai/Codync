package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.zIndex
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray

/** The roster, as iOS `BotListView` draws it: every bot is a person you can message. */
@Composable internal fun BotListView(state: AppState, store: AppStore, account: () -> Unit, manage: () -> Unit,
    newBot: () -> Unit, newGroup: () -> Unit, edit: (Bot) -> Unit, bottomInset: androidx.compose.ui.unit.Dp) {
    var confirmDelete by remember { mutableStateOf<Bot?>(null) }
    var confirmNewSession by remember { mutableStateOf<Bot?>(null) }
    val roster = roster(state.bots)
    val reorder = remember { RosterReorder() }
    reorder.roster = roster.map(Bot::id)
    reorder.lead = roster.firstOrNull { it.lead }?.id
    val online = state.actualConnection is LinkState.Ready
    // A computer that turned this phone away, or this computer's older identity, counts as none (iOS `availableStores`).
    val usable = state.computers.isNotEmpty() && !state.isStale
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    GlassStage(Modifier.fillMaxSize().background(Palette.background), chrome = {
        ScrollEdge(Edge.Top, top + 64.dp)
        Row(Modifier.fillMaxWidth().statusBarsPadding().padding(top = 6.dp).padding(horizontal = 16.dp).height(44.dp),
            verticalAlignment = Alignment.CenterVertically) {
            AccountSwitcherButton(state, account)
            Box(Modifier.weight(1f), contentAlignment = Alignment.Center) { ComputerFilterHeader(state, store, manage) }
            NewMenuButton(enabled = usable, groupEnabled = online, newBot, newGroup)
        }
    }) {
        // Pull to refresh reconnects and asks the account for its computers again (iOS `refreshable`).
        PullToRefresh({ store.reconnect(); store.refreshComputers() }, Modifier.fillMaxSize(), indicatorTop = top + 50.dp) {
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = top + 56.dp, bottom = bottomInset)) {
            // A computer waiting for an update has its card instead; its bots may not be readable yet.
            state.mismatch?.let { mismatch -> item("update") { UpdateNeededCard(state, mismatch, store::requestHostUpdate, Modifier.padding(vertical = 8.dp)) } }
            if (roster.isEmpty() && state.mismatch == null) item {
                EmptyRoster(canCreate = usable, hasComputer = usable, create = newBot, showComputers = manage)
            }
            items(reorder.shown(roster), key = Bot::id) { bot ->
                val lifted = reorder.dragged == bot.id
                Box(Modifier.onSizeChanged { reorder.heights[bot.id] = it.height }
                    .then(if (lifted) Modifier.zIndex(1f).graphicsLayer { translationY = reorder.offset; scaleX = 1.02f; scaleY = 1.02f }
                        else Modifier.animateItem())) {
                    BotRowItem(bot, state, store, reorder.handle(bot.id) { store.reorder(it) }, edit = { edit(bot) },
                        newSession = { confirmNewSession = bot }, delete = { confirmDelete = bot })
                }
            }
        }
        }
    }
    val restarting = confirmNewSession
    CodyncDialog("Start a new session?", restarting != null, { confirmNewSession = null },
        message = "The conversation stays here, but the agent starts with a fresh context.") {
        listOf(DialogAction("New session") { restarting?.let { store.command("newSession", body("botId" to it.id)) } })
    }
    val deleting = confirmDelete
    CodyncDialog("Delete ${deleting?.name ?: "bot"}?", deleting != null, { confirmDelete = null },
        message = if (deleting?.kind == "group") "Its bots and their own chats stay." else "Files it changed on your computer stay as they are.") {
        listOf(DialogAction(if (deleting?.kind == "group") "Delete group chat" else "Delete bot and its conversation", destructive = true) {
            deleting?.let { store.deleteBot(it) {} }
        })
    }
}

/**
 * Drag-to-reorder for the roster (iOS `List.onMove`): long-press a row, then move it. The order
 * shown follows the finger until the host's synced order catches up.
 */
private class RosterReorder {
    var roster: List<String> = emptyList()
    /** Stays on top: nothing takes its place. */
    var lead: String? = null
    var order by mutableStateOf<List<String>?>(null)
    var dragged by mutableStateOf<String?>(null)
    var offset by mutableFloatStateOf(0f)
    val heights = mutableMapOf<String, Int>()
    private val handles = mutableMapOf<String, ContextDrag>()

    fun shown(bots: List<Bot>): List<Bot> {
        val ids = order ?: return bots
        // Dropped and synced: the host's order is the one to show again.
        if (dragged == null && ids == roster) { order = null; return bots }
        return ids.mapNotNull { id -> bots.firstOrNull { it.id == id } } + bots.filter { it.id !in ids }
    }

    fun handle(id: String, save: (List<String>) -> Unit): ContextDrag = handles.getOrPut(id) {
        ContextDrag(start = { order = order ?: roster; dragged = id; offset = 0f }, move = { step -> offset += step; swap(id) }, end = {
            val dropped = order
            dragged = null; offset = 0f
            if (dropped != null && dropped != roster) save(dropped) else order = null
        })
    }

    /** Past half the neighbor's height, the row takes the neighbor's place. */
    private fun swap(id: String) {
        val list = order ?: return
        val at = list.indexOf(id)
        val next = if (offset > 0) at + 1 else at - 1
        if (list.getOrNull(next) == lead) return
        val height = list.getOrNull(next)?.let(heights::get) ?: return
        if (kotlin.math.abs(offset) < height / 2f) return
        order = list.toMutableList().apply { add(next, removeAt(at)) }
        offset -= if (offset > 0) height else -height
    }
}

/** Roster order: the lead, then pinned bots, then where the user dragged them, then most recent activity. */
internal fun roster(bots: List<Bot>): List<Bot> = bots.filter { !it.deleted && !it.hidden }
    .sortedWith(compareBy<Bot> { it.band }.thenBy { it.position }.thenByDescending { it.lastAt })

/** The roster order with a bot one place up (-1) or down (+1) within its band (`Bot.band`), or null when it can't move. */
internal fun moved(roster: List<Bot>, id: String, offset: Int): List<String>? {
    val at = roster.indexOfFirst { it.id == id }
    val neighbor = roster.getOrNull(at + offset)
    if (at < 0 || neighbor == null || neighbor.band != roster[at].band) return null
    return roster.map(Bot::id).toMutableList().apply { this[at] = neighbor.id; this[at + offset] = id }
}

@Composable private fun BotRowItem(bot: Bot, state: AppState, store: AppStore, drag: ContextDrag, edit: () -> Unit, newSession: () -> Unit,
    delete: () -> Unit) {
    // The lead stays on top and in the list: no drag, pin, hide or delete.
    Box(Modifier.fillMaxWidth().contextActions(onClick = { store.openBot(bot.id) }, reorder = drag.takeUnless { bot.lead }, preview = {
        Box(Modifier.padding(horizontal = 8.dp)) { BotRow(bot, state) }
    }) {
        listOfNotNull(
            MenuItem(if (bot.pinned) "Unpin" else "Pin", icon = if (bot.pinned) "pin.slash" else "pin") {
                store.command("updateBot", buildJsonObject { put("id", bot.id); put("pinned", !bot.pinned) }, "roster:" + bot.id)
            }.takeUnless { bot.lead },
            if (bot.kind == "group") MenuItem("Edit group", icon = "person.2", action = edit)
            else MenuItem("Edit profile", icon = "pencil", action = edit),
            MenuItem("Mark as read", icon = "checkmark.message") {
                store.command("markRead", buildJsonObject { put("botId", bot.id) }, "roster:" + bot.id)
            },
            // A group has no agent session of its own.
            MenuItem("New session", icon = "arrow.counterclockwise", action = newSession).takeIf { bot.kind != "group" },
            MenuItem("Hide from list", icon = "eye.slash") {
                store.command("updateBot", buildJsonObject { put("id", bot.id); put("hidden", true) }, "roster:" + bot.id)
            }.takeUnless { bot.lead },
            MenuItem("Delete", icon = "trash", destructive = true, divider = true, action = delete).takeUnless { bot.lead },
        )
    }.semantics {
        contentDescription = bot.name
        // TalkBack's way to reorder (the drag needs a touch and hold, then a move).
        customActions = listOf(-1 to "Move up", 1 to "Move down").map { (offset, title) ->
            CustomAccessibilityAction(title) { moved(roster(state.bots), bot.id, offset)?.let(store::reorder) != null }
        }
    }) {
        BotRow(bot, state)
    }
}

/** One bot: avatar with its status dot, name and day, then the live activity or last message. */
@Composable internal fun BotRow(bot: Bot, state: AppState) {
    Row(Modifier.fillMaxWidth().padding(vertical = 10.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        AvatarWithStatus(bot, state.bots, 46.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (bot.pinned && !bot.lead) Symbol("pin.fill", size = 13.dp, tint = Palette.tertiary)
                    Text(bot.name, Modifier.weight(1f, fill = false), style = Type.body.semibold, color = Palette.text, maxLines = 1,
                        overflow = TextOverflow.Ellipsis)
                }
                Spacer(Modifier.width(8.dp))
                if (bot.lastAt > 0) Text(RelativeTime.day(bot.lastAt, yesterday = tr("Yesterday")), style = Type.subheadline,
                    color = if (bot.unread > 0) Palette.text else Palette.tertiary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Box(Modifier.weight(1f)) { Preview(bot, state) }
                if (bot.unread > 0) Box(Modifier.height(18.dp).widthIn(min = 18.dp).clip(Capsule()).background(Palette.accentFill)
                    .padding(horizontal = 6.dp).semantics { contentDescription = "${bot.unread} unread" }, contentAlignment = Alignment.Center) {
                    Text("${bot.unread}", style = Type.caption2.bold, color = Palette.onAccent)
                }
            }
        }
    }
}

/** Live activity while working, otherwise the last message (Grok Bot row behavior). */
@Composable private fun Preview(bot: Bot, state: AppState) {
    when {
        bot.status == "needsInput" -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            ThinkingOrb(ThinkingOrbState.Listening, 16.dp, Palette.warning)
            Text(tr(bot.activity.ifEmpty { "Needs your approval" }), style = Type.subheadline, color = Palette.warning, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        bot.status == "working" -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            ThinkingOrb(size = 13.dp, color = Palette.secondary)
            Text(tr(bot.activity.ifEmpty { "Working…" }), style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        bot.status == "error" -> Text(messagePreview(bot.lastMessage ?: "Something went wrong"), style = Type.subheadline, color = Palette.danger,
            maxLines = 1, overflow = TextOverflow.Ellipsis)
        else -> Text(messagePreview(bot.lastMessage ?: if (bot.kind == "group") "${bot.members.size} bots"
            else "${backendName(state, bot.backend)} · ${if (bot.managedWorkspace) tr("Personal workspace") else bot.cwd.trimEnd('/').substringAfterLast('/')}"),
            style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** The first non-empty line, without Markdown markers (CodyncKit `MessagePreview`). */
internal fun messagePreview(text: String): String {
    val first = text.lines().map(String::trim).firstOrNull(String::isNotEmpty).orEmpty()
    return first.dropWhile { it in "#>-* " }.replace("**", "").replace("`", "")
}

@Composable private fun EmptyRoster(canCreate: Boolean, hasComputer: Boolean, create: () -> Unit, showComputers: () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(vertical = 40.dp), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(14.dp)) {
        CharacterAvatar("cloud", "green", 72.dp)
        Text("No bots yet", style = Type.title3.semibold, color = Palette.text)
        Text(if (hasComputer) "Create a bot for each kind of work — a reviewer, a fixer, a docs writer — and point it at a project."
            else "Ask one of your account's computers for access, or pair one with its code.",
            style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
        if (canCreate) PrimaryButton("Create your first bot", action = create)
        else if (!hasComputer) PrimaryButton("Computers", action = showComputers)
    }
}

/** The account capsule: who's signed in, opening the account sheet. */
@Composable internal fun AccountSwitcherButton(state: AppState, open: () -> Unit) {
    val (source, pressed) = rememberPress()
    val email = state.account.accounts.firstOrNull { it.userId == state.account.userId }?.email
    Row(Modifier.height(44.dp).glass(Capsule(), pressed = pressed)
        .clickable(source, null, role = Role.Button, onClick = open)
        .semantics { contentDescription = "Switch account"; stateDescription = email ?: "Local pairing" }
        .padding(start = 7.dp, end = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        AccountAvatar(state, 30.dp)
        Symbol("chevron.down", size = 21.dp, tint = Palette.secondary)
    }
}

/** The quiet connection summary, with reconnect and the computers on demand. */
@Composable private fun ComputerFilterHeader(state: AppState, store: AppStore, manage: () -> Unit) {
    val summary = connectionSummary(state)
    DropdownMenu({
        listOf(
            MenuItem("All computers", icon = "checkmark"),
            *state.computers.map { MenuItem("${it.name} · ${connectionLabel(state)}", icon = "checkmark") }.toTypedArray(),
            MenuItem("Reconnect", icon = "arrow.clockwise", divider = true, enabled = state.computers.isNotEmpty()) { store.reconnect() },
            MenuItem("Manage computers", icon = "desktopcomputer", action = manage),
        )
    }, label = "Computers, $summary. Filter conversations") {
        Row(Modifier.padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(summary, style = Type.system(12f).medium, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Symbol("chevron.down", size = 15.dp, tint = Palette.secondary)
        }
    }
}

@Composable private fun NewMenuButton(enabled: Boolean, groupEnabled: Boolean, newBot: () -> Unit, newGroup: () -> Unit) {
    DropdownMenu({
        listOf(MenuItem("New bot", icon = "plus", action = newBot),
            MenuItem("New group chat", icon = "person.2", enabled = groupEnabled, action = newGroup))
    }, Modifier.size(44.dp).glass(Capsule()), enabled = enabled, label = "New") {
        Symbol("plus", size = 30.dp, tint = Palette.text.copy(alpha = if (enabled) 1f else 0.35f))
    }
}

internal fun backendName(state: AppState, id: String): String =
    state.hello?.get("backends")?.let { it as? kotlinx.serialization.json.JsonArray }
        ?.firstOrNull { (it as? kotlinx.serialization.json.JsonObject)?.get("id") == JsonPrimitive(id) }
        ?.let { ((it as kotlinx.serialization.json.JsonObject)["name"] as? JsonPrimitive)?.content }
        ?: when (id) { "claude" -> "Claude Code"; "codex" -> "Codex"; "opencode" -> "OpenCode"; "grok" -> "Grok Build"; "gemini" -> "Gemini CLI"; else -> "Custom agent" }

/** Saves a dragged roster order (the host syncs it to every device). */
internal fun AppStore.reorder(ids: List<String>) =
    command("reorderBots", buildJsonObject { putJsonArray("ids") { ids.forEach { add(JsonPrimitive(it)) } } }, "reorder")
