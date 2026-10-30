package com.codync.android

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
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
    Text(RelativeTime.separator(at, tr("Today"), tr("Yesterday")), Modifier.fillMaxWidth().padding(top = 18.dp, bottom = 6.dp), style = Type.footnote,
        color = Palette.tertiary, textAlign = TextAlign.Center)
}

/** One chat-visible entry, with what can be done to it. `reply` quotes it in the composer; `addFallback` opens a bot's fallbacks;
 *  `openArtifact` opens an artifact card's artifact. */
@Composable internal fun ChatRow(entry: Entry, groupStart: Boolean, chat: Bot?, state: AppState, store: AppStore,
    openTrace: () -> Unit, reply: (() -> Unit)?, connect: ConnectionRequestActions, addFallback: (Bot) -> Unit, openArtifact: (OpenArtifact) -> Unit) {
    val group = chat?.kind == "group"
    when (entry.kind) {
        "user" -> Column(Modifier.fillMaxWidth().padding(top = if (groupStart) 12.dp else 4.dp).animateContentSize(Motion.layoutSize),
            horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
            UserBubble(entry, state, store, reply)
            ReactionsRow(entry, state, store)
        }
        "agent" -> Column(Modifier.fillMaxWidth().padding(top = if (groupStart) 12.dp else 4.dp).animateContentSize(Motion.layoutSize),
            verticalArrangement = Arrangement.spacedBy(4.dp)) {
            if (group && groupStart) AuthorLabel(entry.data.text("author"), state)
            AgentBubble(entry, state, store, openTrace, reply, openArtifact)
            Box(Modifier.padding(start = if (group) 34.dp else 0.dp)) { ReactionsRow(entry, state, store) }
        }
        "permission" -> Column(Modifier.padding(top = 12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            if (group) AuthorLabel(entry.data.text("author"), state)
            PermissionCard(entry, state.computers.firstOrNull()?.name.orEmpty(), answering = "permission:" + entry.id in state.busy) {
                store.respond(entry, it)
            }
        }
        else -> if (entry.data["connectionRequest"] is JsonObject) Box(Modifier.padding(top = 12.dp)) {
            ConnectionRequestCard(entry, state, store, connect.openApp, connect.install)
        } else Box(Modifier.padding(top = 10.dp)) {
            // In a group, the member whose agent failed.
            val owner = state.bots.firstOrNull { it.id == (entry.data.text("author") ?: entry.botId) && it.kind != "group" }
            NoticeRow(entry, owner?.let { { addFallback(it) } })
        }
    }
}

/** What a connection request in the chat opens. */
internal class ConnectionRequestActions(val openApp: (String, ComposioApp) -> Unit, val install: (String, MarketConnector) -> Unit)

/** Who wrote a message: their avatar and name in their color, or [name] for a bot on another computer. */
@Composable internal fun AuthorLabel(botId: String?, state: AppState, name: String? = null) {
    val bot = state.bots.firstOrNull { it.id == botId }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (bot != null) CharacterAvatar(bot.avatarShape, bot.avatarColor, 26.dp, customImage = bot.customIcon?.dataUrl, customTinted = bot.customIcon?.tinted == true)
        Text(state.authorName(botId, name), style = Type.subheadline.medium, color = bot?.let { AvatarPalette.tint(it.avatarColor) } ?: Palette.secondary)
    }
}

/** The menu a message opens on long press: copy, reply quoting it, what the agent did. */
private fun messageMenu(context: android.content.Context, entry: Entry, reply: (() -> Unit)?, trace: (() -> Unit)?) = buildList {
    add(MenuItem("Copy", icon = "square.on.square") {
        context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Message", entry.text))
    })
    if (reply != null) add(MenuItem("Reply", icon = "arrowshape.turn.up.left", action = reply))
    if (trace != null) add(MenuItem("Show what it did", icon = "list.bullet", action = trace))
}

internal val quickReactions = listOf("👍", "❤️", "😂", "🎉", "👀", "✅")

private fun reactionPick(entry: Entry, store: AppStore) = ReactionPick(quickReactions, entry.reactions) { emoji ->
    store.command("react", body("entryId" to entry.id, "emoji" to emoji), "react:" + entry.id)
}

internal val Entry.reactions: List<String> get() = (data["reactions"] as? JsonArray)?.map { it.jsonPrimitive.content }.orEmpty()

@Composable private fun UserBubble(entry: Entry, state: AppState, store: AppStore, reply: (() -> Unit)?) {
    val context = LocalContext.current
    val focus = LocalFocusManager.current
    val attachments = entry.data["attachments"]?.jsonArray?.map { WireJson.decodeFromJsonElement<Attachment>(it) }.orEmpty()
    Column(Modifier.fillMaxWidth().padding(start = 56.dp), horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (attachments.isNotEmpty()) AttachmentList(attachments, entry.botId, state, store)
        val quote = entry.replyTo
        if (entry.text.isNotEmpty() || quote != null) Box(Modifier.clip(RoundedRectangle(18.dp)).background(Palette.bubbleUser)
            .contextActions(reactions = reactionPick(entry, store), onClick = { focus.clearFocus() }, preview = { SelectionContainer { UserText(entry.text, quote) } }) {
                messageMenu(context, entry, reply, null)
            }) { UserText(entry.text, quote) }
        SendStatusLine(entry.status, state.bots.firstOrNull { it.id == entry.botId }?.status == "working")
    }
}

@Composable private fun UserText(text: String, quote: ReplyQuote? = null) {
    Column(Modifier.background(Palette.bubbleUser).padding(horizontal = 16.dp, vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (quote != null) QuoteLine(quote.text)
        if (text.isNotEmpty()) Text(text, style = Type.body, color = Palette.text)
    }
}

/** The message a reply quotes, as its first line without Markdown markers: in its bubble, and in the composer while replying. */
@Composable internal fun QuoteLine(text: String, modifier: Modifier = Modifier) {
    Row(modifier.semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Symbol("text.bubble", size = 14.dp, tint = Palette.secondary)
        Text("“${messagePreview(text)}”", style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
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
        if (send.text.isNotEmpty() || send.replyTo != null) Box(Modifier.clip(RoundedRectangle(18.dp))) { UserText(send.text, send.replyTo) }
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

@Composable private fun AgentBubble(entry: Entry, state: AppState, store: AppStore, openTrace: () -> Unit, reply: (() -> Unit)?,
    openArtifact: (OpenArtifact) -> Unit) {
    val context = LocalContext.current
    val focus = LocalFocusManager.current
    val files = entry.files
    val artifact = entry.artifact
    Column(Modifier.fillMaxWidth().padding(end = 40.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        // An artifact card or a message with files is that: the text is only the computer's fallback.
        if (artifact != null) ArtifactCardView(artifact, entry.botId, state, store, Modifier.contextActions(reactions = reactionPick(entry, store),
            onClick = { focus.clearFocus(); openArtifact(OpenArtifact(entry.botId, artifact.id, artifact.title, entry.id)) }) { messageMenu(context, entry, reply, openTrace) })
        else if (files.isNotEmpty()) Column(Modifier.contextActions(reactions = reactionPick(entry, store), onClick = { focus.clearFocus() }) { messageMenu(context, entry, reply, openTrace) },
            verticalArrangement = Arrangement.spacedBy(4.dp)) {
            for (file in files) FileDownloadCard(entry, file, state, store)
        } else if (entry.text.isNotEmpty()) Box(Modifier.clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent)
            .contextActions(reactions = reactionPick(entry, store), onClick = { focus.clearFocus() }, preview = { AgentText(entry.text, selectable = true) }) {
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

/** A message's reactions: the user's (tapping one takes it back), then the bots', each emoji with the avatars of the
 *  bots that chose it. */
@Composable private fun ReactionsRow(entry: Entry, state: AppState, store: AppStore) {
    val mine = entry.reactions
    val theirs = entry.botReactions.mapValues { (_, ids) -> ids.mapNotNull { id -> state.bots.firstOrNull { it.id == id } } }
        .filterValues { it.isNotEmpty() }
    if (mine.isEmpty() && theirs.isEmpty()) return
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        for (emoji in mine) {
            val (source, pressed) = rememberPress()
            Text(emoji, Modifier.pressScale(pressed).clip(Capsule()).background(Palette.surface)
                .clickable(source, null, role = Role.Button) { store.command("react", body("entryId" to entry.id, "emoji" to emoji), "react:" + entry.id) }
                .semantics { contentDescription = "Remove reaction $emoji" }
                .padding(horizontal = 8.dp, vertical = 3.dp), style = Type.system(15f))
        }
        for ((emoji, bots) in theirs) {
            Row(Modifier.clip(Capsule()).background(Palette.surface)
                .clearAndSetSemantics { contentDescription = "${bots.joinToString { it.name }} reacted $emoji" }
                .padding(horizontal = 8.dp, vertical = 3.dp), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(emoji, style = Type.system(15f))
                Row(horizontalArrangement = Arrangement.spacedBy((-5).dp)) {
                    for (bot in bots.take(3)) CharacterAvatar(bot.avatarShape, bot.avatarColor, 16.dp, customImage = bot.customIcon?.dataUrl, customTinted = bot.customIcon?.tinted == true)
                }
            }
        }
    }
}

/** The bots' reactions by emoji, in the order each first appeared: each emoji with the bots that chose it. */
internal val Entry.botReactions: Map<String, List<String>> get() =
    (data["botReactions"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }
        .groupBy({ it["emoji"]?.jsonPrimitive?.contentOrNull.orEmpty() }, { it["bot"]?.jsonPrimitive?.contentOrNull.orEmpty() })

/** A routine's line in the main chat: tapping it opens Routines on that routine; a failed run the user can run again
 *  has a retry button. */
@Composable internal fun RoutineNotice(entry: Entry, retrying: Boolean, retry: () -> Unit, open: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.plainButton(action = open).padding(vertical = 10.dp), verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Symbol("clock.arrow.circlepath", size = 15.dp, tint = Palette.secondary)
            Text(entry.text.ifEmpty { "Routine" }, style = Type.footnote, color = Palette.secondary)
        }
        if (entry.data["retry"]?.jsonPrimitive?.booleanOrNull == true) {
            if (retrying) Spinner(14.dp)
            else IconButton("Try again", "arrow.clockwise", action = retry)
        }
    }
}

/** A line from the computer. `addFallback`, on a failure the bot's fallbacks could have carried, opens them. */
@Composable private fun NoticeRow(entry: Entry, addFallback: (() -> Unit)?) {
    val text = entry.text
    val seconds = entry.data["callSeconds"]?.jsonPrimitive?.longOrNull
    when {
        seconds != null -> Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
            Symbol("waveform", size = 15.dp, tint = Palette.secondary)
            Spacer(Modifier.width(6.dp))
            Text("$text · %02d:%02d".format(seconds / 60, seconds % 60), style = Type.footnote.monospacedDigit, color = Palette.secondary)
        }
        entry.data.text("style") == "error" -> Row(Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp))
            .background(Palette.danger.copy(alpha = 0.12f)).padding(12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Symbol("exclamationmark.triangle.fill", size = 16.dp, tint = Palette.danger)
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                SelectionContainer { Text(text, style = Type.footnote, color = Palette.text) }
                if (addFallback != null && entry.data["addFallback"]?.jsonPrimitive?.booleanOrNull == true) SecondaryButton("Add a fallback", action = addFallback)
            }
        }
        "agentSwitch" in entry.data -> AgentSwitchLine(text, (entry.data["agentSwitch"] as? JsonObject)?.text("carried"))
        // A fresh conversation starts at the left, as on the iPhone.
        "handoff" in entry.data -> Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Symbol("arrow.triangle.branch", size = 13.dp, tint = Palette.secondary)
            Text(text, style = Type.footnote, color = Palette.secondary)
        }
        else -> Text(text, Modifier.fillMaxWidth(), style = Type.footnote, color = Palette.secondary, textAlign = TextAlign.Center)
    }
}

/** Another agent took over the chat (or its own is back): tapping the line unfolds what the new agent was told. */
@Composable private fun AgentSwitchLine(text: String, carried: String?) {
    val label: @Composable RowScope.() -> Unit = {
        Symbol("arrow.turn.down.right", size = 13.dp, tint = Palette.secondary)
        Spacer(Modifier.width(6.dp))
        Text(text, Modifier.weight(1f, fill = false), style = Type.footnote, color = Palette.secondary)
    }
    var expanded by remember { mutableStateOf(false) }
    if (carried.isNullOrEmpty()) Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, content = label)
    else Disclosure(expanded, { expanded = !expanded }, Modifier.fillMaxWidth(), label) {
        SelectionContainer {
            Text(carried, Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.bubbleAgent).padding(12.dp),
                style = Type.footnote, color = Palette.secondary)
        }
    }
}

/** The bot's current activity and elapsed time; with thinking `details` on (Settings), a tap unfolds the turn's steps. */
@Composable internal fun WorkingIndicator(bot: Bot, steps: List<Entry>, details: Boolean) {
    val needs = bot.status == "needsInput"
    // Each turn starts folded.
    var expanded by remember(bot.startedAt) { mutableStateOf(false) }
    val open = details && expanded
    val turn by animateFloatAsState(if (open) 90f else 0f, Motion.layout(), label = "Chevron")
    val action = tr(if (open) "Hide steps" else "Show steps")
    val state = tr(if (open) "Expanded" else "Collapsed")
    val toggle = if (!details) Modifier
        else Modifier.clickable(role = Role.Button, onClickLabel = action) { expanded = !expanded }.semantics { stateDescription = state }
    Column(Modifier.fillMaxWidth().padding(top = 6.dp, end = 40.dp)) {
        Column(Modifier.then(if (open) Modifier.fillMaxWidth() else Modifier).animateContentSize(Motion.layoutSize)
            .clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent).padding(horizontal = 16.dp, vertical = 12.dp)) {
            Row(Modifier.then(if (open) Modifier.fillMaxWidth() else Modifier).then(toggle),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                ThinkingOrb(if (needs) ThinkingOrbState.Listening else ThinkingOrbState.Working, 16.dp, if (needs) Palette.warning else Palette.secondary)
                Text(tr(bot.activity.ifEmpty { "Working…" }), Modifier.weight(1f, fill = open), style = Type.subheadline,
                    color = if (needs) Palette.warning else Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                bot.startedAt?.let { Elapsed(it) }
                if (details) Symbol("chevron.right", Modifier.rotate(turn), size = 13.dp, tint = Palette.tertiary)
            }
            AnimatedVisibility(open, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
                exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) { WorkingSteps(steps) }
        }
    }
}

/** The turn's thinking, narration, tool calls and plan: hugs them, scrolls past 280 dp, and follows new steps until
 *  the reader scrolls up. A turn can have none yet: Claude's thinking text arrives as its thinking ends. */
@Composable private fun WorkingSteps(steps: List<Entry>) {
    val scroll = rememberScrollState()
    var follow by remember { mutableStateOf(true) }
    LaunchedEffect(scroll.isScrollInProgress) { if (!scroll.isScrollInProgress) follow = !scroll.canScrollForward }
    // Follows new steps once they're laid out, not a row the reader unfolds (that grows the box too).
    LaunchedEffect(steps) {
        if (!follow) return@LaunchedEffect
        withFrameNanos {}
        scroll.scrollTo(scroll.maxValue)
    }
    Column(Modifier.padding(top = 12.dp).heightIn(max = 280.dp).verticalScroll(scroll), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (steps.isEmpty()) Text(tr("Nothing yet."), style = Type.body, color = Palette.tertiary)
        for (entry in steps) key(entry.id) { TraceRow(entry, unfolded = true) }
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

/** The start of an empty chat: who the bot is and how it works. */
@Composable internal fun IntroCard(bot: Bot, state: AppState) {
    val members = bot.memberBots(state.bots)
    Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp).padding(top = 40.dp), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp)) {
        BotAvatar(bot, state.bots, 72.dp)
        Text(bot.name, style = Type.title2.semibold, color = Palette.text)
        val lines = if (bot.kind == "group") listOfNotNull(
            members.joinToString(" · ") { it.name }.takeIf { bot.name != members.joinToString(", ") { it.name } },
            bot.description.ifEmpty { null }) else listOfNotNull(
            if (bot.managedWorkspace) "${backendName(state, bot.backend)} · ${tr("Personal workspace")}"
            else tr("{agent} in {path}").replace("{agent}", backendName(state, bot.backend)).replace("{path}", bot.cwd),
            bot.description.ifEmpty { null })
        for (line in lines) Text(line, style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
        Text(if (bot.kind == "group") "Everyone answers in turn. @mention a bot to ask just that one."
            else "Tell it what you need. You'll get a notification when it's done or needs you.",
            Modifier.padding(top = 4.dp), style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
    }
}

internal fun JsonObject.text(key: String): String? = this[key]?.jsonPrimitive?.contentOrNull
