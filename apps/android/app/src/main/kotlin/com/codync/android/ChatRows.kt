package com.codync.android

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

/** One row of a conversation, as iOS `ChatItem` builds them. */
internal sealed interface ChatItem {
    val id: String
    data class Separator(override val id: String, val at: Long) : ChatItem
    data class Message(override val id: String, val entry: Entry, val groupStart: Boolean) : ChatItem
    data class Pending(override val id: String, val send: PendingSend, val groupStart: Boolean) : ChatItem
    /** Consecutive bot-to-bot exchanges with one peer, as one line. */
    data class Exchanges(override val id: String, val group: BotExchangeGroup) : ChatItem
}

/** Chat-visible entries plus time separators (gaps over an hour) and author grouping. */
internal fun chatItems(entries: List<Entry>, pending: List<PendingSend>): List<ChatItem> {
    val out = mutableListOf<ChatItem>()
    var last: Long? = null
    var author: String? = null
    // Exchanges with the same peer in a row collapse into one line; anything shown in between ends the run.
    val units = mutableListOf<Pair<Long, Any>>()
    var run = mutableListOf<BotExchange>()
    fun flush() { if (run.isNotEmpty()) { units += run.first().createdAt to BotExchangeGroup(run); run = mutableListOf() } }
    for (entry in entries) {
        val exchange = BotExchange.of(entry)
        if (exchange == null) { flush(); units += entry.createdAt to entry; continue }
        if (run.isNotEmpty() && run.last().peerId != exchange.peerId) flush()
        run += exchange
    }
    flush()
    val slots = units + pending.filter { send -> entries.none { it.clientNonce == send.nonce } }.map { it.createdAt to it }
    for ((at, slot) in slots.sortedBy { it.first }) {
        val id = when (slot) { is Entry -> slot.clientNonce?.takeIf { slot.kind == "user" && it.isNotEmpty() }?.let { "user-$it" } ?: slot.id
            is BotExchangeGroup -> "x-" + slot.id
            else -> "user-" + (slot as PendingSend).nonce }
        if (last == null || at - last > 3_600_000) { out += ChatItem.Separator("sep-$id", at); author = null }
        last = at
        when (slot) {
            is Entry -> {
                val who = when (slot.kind) { "user" -> "user"; "agent" -> "agent:" + slot.data.text("author").orEmpty(); else -> slot.kind }
                out += ChatItem.Message(id, slot, who != author)
                author = if (who == "user" || slot.kind == "agent") who else null
            }
            is PendingSend -> { out += ChatItem.Pending(id, slot, author != "user"); author = "user" }
            is BotExchangeGroup -> { out += ChatItem.Exchanges(id, slot); author = null }
        }
    }
    return out
}

/** A chat's time separator: Today 3:27 AM. */
@Composable internal fun TimeSeparator(at: Long) {
    Text(RelativeTime.separator(at), Modifier.fillMaxWidth().padding(top = 18.dp, bottom = 6.dp), style = Type.footnote,
        color = Palette.tertiary, textAlign = TextAlign.Center)
}

/** One chat-visible entry, with what can be done to it. `reply` opens its thread (main chat only). */
@Composable internal fun ChatRow(entry: Entry, groupStart: Boolean, chat: Bot?, state: AppState, store: AppStore,
    openTrace: () -> Unit, reply: (() -> Unit)?, connect: ConnectionRequestActions) {
    val group = chat?.kind == "group"
    when (entry.kind) {
        "user" -> Column(Modifier.fillMaxWidth().padding(top = if (groupStart) 12.dp else 4.dp).animateContentSize(Motion.layoutSize),
            horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
            UserBubble(entry, state, store, reply)
            ReactionsRow(entry, store)
            ThreadChip(entry, state, reply)
        }
        "agent" -> Column(Modifier.fillMaxWidth().padding(top = if (groupStart) 12.dp else 4.dp).animateContentSize(Motion.layoutSize),
            verticalArrangement = Arrangement.spacedBy(4.dp)) {
            if (group && groupStart) AuthorLabel(entry.data.text("author"), state)
            AgentBubble(entry, state, store, openTrace, reply)
            Column(Modifier.padding(start = if (group) 34.dp else 0.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                ReactionsRow(entry, store)
                ThreadChip(entry, state, reply)
            }
        }
        "permission" -> Column(Modifier.padding(top = 12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            if (group) AuthorLabel(entry.data.text("author"), state)
            PermissionCard(entry, state.computers.firstOrNull()?.name.orEmpty(), answering = "permission:" + entry.id in state.busy) {
                store.respond(entry, it)
            }
        }
        else -> if (entry.data["connectionRequest"] is JsonObject) Box(Modifier.padding(top = 12.dp)) {
            ConnectionRequestCard(entry, state, store, connect.openApp, connect.install)
        } else Box(Modifier.padding(top = 10.dp)) { NoticeRow(entry) }
    }
}

/** What a connection request in the chat opens. */
internal class ConnectionRequestActions(val openApp: (String, ComposioApp) -> Unit, val install: (String, MarketConnector) -> Unit)

/** Who wrote a message in a group: their avatar and name in their color. */
@Composable internal fun AuthorLabel(botId: String?, state: AppState) {
    val bot = state.bots.firstOrNull { it.id == botId }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (bot != null) CharacterAvatar(bot.avatarShape, bot.avatarColor, 26.dp)
        Text(bot?.name ?: "A deleted bot", style = Type.subheadline.medium, color = bot?.let { AvatarPalette.tint(it.avatarColor) } ?: Palette.secondary)
    }
}

/** The menu a message opens on long press: copy, reply in its thread, what the agent did. */
private fun messageMenu(context: android.content.Context, entry: Entry, reply: (() -> Unit)?, trace: (() -> Unit)?) = buildList {
    add(MenuItem("Copy", icon = "square.on.square") {
        context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Message", entry.text))
    })
    if (reply != null) add(MenuItem("Reply in thread", icon = "arrowshape.turn.up.left", action = reply))
    if (trace != null) add(MenuItem("Show what it did", icon = "list.bullet", action = trace))
}

internal val quickReactions = listOf("👍", "❤️", "😂", "🎉", "👀", "✅")

private fun reactionPick(entry: Entry, store: AppStore) = ReactionPick(quickReactions, entry.reactions) { emoji ->
    store.command("react", body("entryId" to entry.id, "emoji" to emoji), "react:" + entry.id)
}

internal val Entry.reactions: List<String> get() = (data["reactions"] as? JsonArray)?.map { it.jsonPrimitive.content }.orEmpty()

@Composable private fun UserBubble(entry: Entry, state: AppState, store: AppStore, reply: (() -> Unit)?) {
    val context = LocalContext.current
    val attachments = entry.data["attachments"]?.jsonArray?.map { WireJson.decodeFromJsonElement<Attachment>(it) }.orEmpty()
    Column(Modifier.fillMaxWidth().padding(start = 56.dp), horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (attachments.isNotEmpty()) AttachmentList(attachments, entry.botId, state, store)
        if (entry.text.isNotEmpty()) Box(Modifier.clip(RoundedRectangle(18.dp)).background(Palette.bubbleUser)
            .contextActions(reactions = reactionPick(entry, store), preview = { SelectionContainer { UserText(entry.text) } }) {
                messageMenu(context, entry, reply, null)
            }) { UserText(entry.text) }
        SendStatusLine(entry.status, state.bots.firstOrNull { it.id == entry.botId }?.status == "working")
    }
}

@Composable private fun UserText(text: String) {
    Text(text, Modifier.background(Palette.bubbleUser).padding(horizontal = 16.dp, vertical = 10.dp), style = Type.body, color = Palette.text)
}

@Composable private fun SendStatusLine(status: String?, botWorking: Boolean) {
    val text = when (status) {
        "sending" -> "Sending…"
        "queued" -> if (botWorking) "Queued until this response finishes" else "Queued"
        "cancelled" -> "Not sent — stopped"
        "delivering" -> "Delivered to the computer"
        else -> null
    }
    if (text != null) Text(text, style = Type.caption2, color = Palette.tertiary)
}

/** A message this phone is still delivering: the same bubble, with how far it got. */
@Composable internal fun PendingBubble(send: PendingSend, groupStart: Boolean, state: AppState, store: AppStore) {
    Column(Modifier.fillMaxWidth().padding(top = if (groupStart) 12.dp else 4.dp).padding(start = 56.dp),
        horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        for (file in send.files) Text(file.name + " · " + formatBytes(file.size), Modifier.clip(RoundedRectangle(14.dp))
            .background(Palette.surface).padding(horizontal = 12.dp, vertical = 8.dp), style = Type.footnote, color = Palette.text)
        if (send.text.isNotEmpty()) Box(Modifier.clip(RoundedRectangle(18.dp))) { UserText(send.text) }
        when (send.status) {
            SendStatus.Sending -> Text(state.uploadProgress[send.nonce]?.let { "Uploading files · $it%" } ?: "Sending…",
                style = Type.caption2, color = Palette.tertiary)
            SendStatus.Delivering -> Text("Delivered to the computer", style = Type.caption2, color = Palette.tertiary)
            SendStatus.Waiting -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Text("Waiting for the computer to come online", style = Type.caption2, color = Palette.tertiary)
                Box(Modifier.clickable(role = Role.Button) { store.cancelQueued(send) }.semantics { contentDescription = "Don't send" }) {
                    Symbol("xmark.circle", size = 14.dp, tint = Palette.tertiary)
                }
            }
            SendStatus.Failed -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(send.error ?: "Failed to send", style = Type.caption2.bold, color = Palette.danger)
                Box(Modifier.clickable(role = Role.Button) { store.retry(send) }.semantics { contentDescription = "Resend" }) {
                    Symbol("arrow.clockwise", size = 14.dp, tint = Palette.text)
                }
                Box(Modifier.clickable(role = Role.Button) { store.discard(send) }.semantics { contentDescription = "Delete" }) {
                    Symbol("trash", size = 14.dp, tint = Palette.text)
                }
            }
        }
    }
}

@Composable private fun AgentBubble(entry: Entry, state: AppState, store: AppStore, openTrace: () -> Unit, reply: (() -> Unit)?) {
    val context = LocalContext.current
    val files = entry.files
    Column(Modifier.fillMaxWidth().padding(end = 40.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        // A message with files is its files: the text is only the computer's fallback.
        if (files.isNotEmpty()) Column(Modifier.contextActions(reactions = reactionPick(entry, store)) { messageMenu(context, entry, reply, openTrace) },
            verticalArrangement = Arrangement.spacedBy(4.dp)) {
            for (file in files) FileDownloadCard(entry, file, state, store)
        } else if (entry.text.isNotEmpty()) Box(Modifier.clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent)
            .contextActions(reactions = reactionPick(entry, store), preview = { AgentText(entry.text, selectable = true) }) {
                messageMenu(context, entry, reply, openTrace)
            }) { AgentText(entry.text, selectable = false) }
    }
}

/**
 * A reply. Holding it opens its menu over a lifted copy, as on iOS, so only that copy is `selectable`:
 * text selection takes the same long press.
 */
@Composable private fun AgentText(text: String, selectable: Boolean) {
    Box(Modifier.background(Palette.bubbleAgent).padding(horizontal = 16.dp, vertical = 10.dp)) { MarkdownText(text, selectable) }
}

/** Under a message with a thread (Slack's): how many replied, how recently. */
@Composable private fun ThreadChip(entry: Entry, state: AppState, open: (() -> Unit)?) {
    val summary = entry.data["thread"] as? JsonObject ?: return
    val count = summary["count"]?.jsonPrimitive?.intOrNull ?: 0
    if (count == 0 || open == null) return
    val unread = summary["unread"]?.jsonPrimitive?.intOrNull ?: 0
    val lastAt = summary["lastAt"]?.jsonPrimitive?.longOrNull ?: 0
    val authors = (summary["authors"] as? JsonArray)?.mapNotNull { id -> state.bots.firstOrNull { it.id == id.jsonPrimitive.content } }.orEmpty()
    val (source, pressed) = rememberPress()
    Row(Modifier.pressScale(pressed).clip(Capsule()).background(Palette.surface)
        .clickable(source, null, role = Role.Button, onClick = open)
        .semantics { contentDescription = "View thread, $count ${if (count == 1) "reply" else "replies"}" + if (unread > 0) ", $unread new" else "" }
        .padding(horizontal = 10.dp, vertical = 5.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy((-6).dp)) {
            for (bot in authors.take(3)) CharacterAvatar(bot.avatarShape, bot.avatarColor, 18.dp)
        }
        Text(if (count == 1) "1 reply" else "$count replies", style = Type.footnote.semibold, color = Palette.accent)
        if (unread > 0) Text("$unread new", style = Type.footnote.semibold, color = Palette.text)
        else if (lastAt > 0) Text(RelativeTime.day(lastAt), style = Type.footnote, color = Palette.tertiary)
        Symbol("chevron.right", size = 12.dp, tint = Palette.tertiary)
    }
}

/** The user's reactions under a message; tapping one takes it back. */
@Composable private fun ReactionsRow(entry: Entry, store: AppStore) {
    val reactions = entry.reactions
    if (reactions.isEmpty()) return
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        for (emoji in reactions) {
            val (source, pressed) = rememberPress()
            Text(emoji, Modifier.pressScale(pressed).clip(Capsule()).background(Palette.surface)
                .clickable(source, null, role = Role.Button) { store.command("react", body("entryId" to entry.id, "emoji" to emoji), "react:" + entry.id) }
                .semantics { contentDescription = "Remove reaction $emoji" }
                .padding(horizontal = 8.dp, vertical = 3.dp), style = Type.system(15f))
        }
    }
}

/** A routine's run in the main chat: tapping it opens Routines on that routine. */
@Composable internal fun RoutineNotice(entry: Entry, open: () -> Unit) {
    Row(Modifier.plainButton(action = open).padding(vertical = 10.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Symbol("clock.arrow.circlepath", size = 15.dp, tint = Palette.secondary)
        Text(entry.text.ifEmpty { "Routine" }, style = Type.footnote, color = Palette.secondary)
    }
}

@Composable private fun NoticeRow(entry: Entry) {
    val text = entry.text
    val seconds = entry.data["callSeconds"]?.jsonPrimitive?.longOrNull
    when {
        seconds != null -> Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
            Symbol("waveform", size = 15.dp, tint = Palette.secondary)
            Spacer(Modifier.width(6.dp))
            Text("$text · %02d:%02d".format(seconds / 60, seconds % 60), style = Type.footnote.monospacedDigit, color = Palette.secondary)
        }
        entry.data.text("style") == "divider" -> Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Box(Modifier.weight(1f).height(1.dp).background(Palette.border))
            Text(text, Modifier.widthIn(max = 260.dp), style = Type.caption2, color = Palette.tertiary, textAlign = TextAlign.Center)
            Box(Modifier.weight(1f).height(1.dp).background(Palette.border))
        }
        entry.data.text("style") == "error" -> Row(Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp))
            .background(Palette.danger.copy(alpha = 0.12f)).padding(12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Symbol("exclamationmark.triangle.fill", size = 16.dp, tint = Palette.danger)
            SelectionContainer { Text(text, style = Type.footnote, color = Palette.text) }
        }
        else -> Text(text, Modifier.fillMaxWidth(), style = Type.footnote, color = Palette.secondary, textAlign = TextAlign.Center)
    }
}

/**
 * The bot's live activity line: a turning orb, what it's doing and for how long. Tapping it
 * unfolds what the bot is thinking right now.
 */
@Composable internal fun WorkingIndicator(bot: Bot, thinking: String?) {
    var expanded by remember { mutableStateOf(false) }
    val needs = bot.status == "needsInput"
    val turn by animateFloatAsState(if (expanded) 180f else 0f, Motion.layout(), label = "Thinking")
    Row(Modifier.fillMaxWidth().padding(top = 6.dp, end = 40.dp)) {
        Column(Modifier.clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent)
            .clickable(enabled = thinking != null, role = Role.Button) { expanded = !expanded }
            .animateContentSize(Motion.layoutSize).padding(horizontal = 16.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                ThinkingOrb(if (needs) ThinkingOrbState.Listening else ThinkingOrbState.Working, 16.dp, if (needs) Palette.warning else Palette.secondary)
                Text(bot.activity.ifEmpty { "Working…" }, Modifier.weight(1f, fill = false), style = Type.subheadline,
                    color = if (needs) Palette.warning else Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                bot.startedAt?.let { Elapsed(it) }
                if (thinking != null) Symbol("chevron.down", Modifier.rotate(turn), size = 12.dp, tint = Palette.tertiary)
            }
            if (expanded && thinking != null) Box(Modifier.heightIn(max = 180.dp).verticalScroll(rememberScrollState(Int.MAX_VALUE))) {
                SelectionContainer { Text(thinking, style = Type.footnote, color = Palette.secondary) }
            }
        }
    }
}

/** `Text(date, style: .timer)`: the time since `startedAt`, every second. */
@Composable private fun Elapsed(startedAt: Long) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(startedAt) { while (true) { now = System.currentTimeMillis(); delay(1000) } }
    val seconds = maxOf(0, (now - startedAt) / 1000)
    Text(if (seconds >= 3600) "%d:%02d:%02d".format(seconds / 3600, seconds / 60 % 60, seconds % 60) else "%d:%02d".format(seconds / 60, seconds % 60),
        style = Type.footnote.monospacedDigit, color = Palette.tertiary)
}

/** The latest thinking of the turn running in a chat (`thread` null) or thread. */
internal fun currentThinking(entries: List<Entry>, botId: String, thread: String?): String? {
    for (e in entries.asReversed()) {
        if (e.botId != botId || e.threadId != thread) continue
        if (e.kind == "user" || (e.kind == "agent" && e.data["final"]?.jsonPrimitive?.booleanOrNull == true)) return null
        if (e.kind == "thought" && e.text.isNotEmpty()) return e.text
    }
    return null
}

/** The start of an empty chat: who the bot is and how it works. */
@Composable internal fun IntroCard(bot: Bot, state: AppState) {
    val members = bot.members.mapNotNull { id -> state.bots.firstOrNull { it.id == id } }
    Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp).padding(top = 40.dp), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp)) {
        BotAvatar(bot, state.bots, 72.dp)
        Text(bot.name, style = Type.title2.semibold, color = Palette.text)
        val lines = if (bot.kind == "group") listOfNotNull(
            members.joinToString(" · ") { it.name }.takeIf { bot.name != members.joinToString(", ") { it.name } },
            bot.description.ifEmpty { null }) else listOfNotNull(
            if (bot.managedWorkspace) "${backendName(state, bot.backend)} · Personal workspace" else "${backendName(state, bot.backend)} in ${bot.cwd}",
            bot.description.ifEmpty { null })
        for (line in lines) Text(line, style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
        Text(if (bot.kind == "group") "Everyone answers in turn. @mention a bot to ask just that one."
            else "Tell it what you need. You'll get a notification when it's done or needs you.",
            Modifier.padding(top = 4.dp), style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
    }
}

internal fun JsonObject.text(key: String): String? = this[key]?.jsonPrimitive?.contentOrNull
