package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle

/**
 * The messages waiting behind the running turn, as iOS `QueuedMessages` and the desktop `QueuedStrip`: the next one
 * above the message field with Edit, Steer and Delete, and the whole queue in a sheet. Steer hands one to the agent
 * now (host `steer`) instead of after the turn; Edit and Delete only change what waits.
 */

/** The messages waiting in this conversation's lane, oldest first. */
internal fun AppState.queuedMessages(botId: String, thread: String?): List<Entry> =
    entries.filter { it.botId == botId && it.threadId == thread && it.kind == "user" && it.status == "queued" }

/** The next waiting message over the field; a steered one leaves the queue and its own turn.
 *  `working`: the agent is answering this lane, so Steer can reach it. */
@Composable internal fun QueuedStrip(entries: List<Entry>, working: Boolean, store: AppStore, open: () -> Unit) {
    val next = entries.firstOrNull()
    var editing by remember { mutableStateOf<Entry?>(null) }
    Column {
        AnimatedVisibility(next != null, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
            exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
            next?.let { entry ->
                val spoken = tr("Waiting to be sent") + ": " + entry.quotePreview
                Column(Modifier.padding(horizontal = 16.dp, vertical = 4.dp).clip(RoundedRectangle(18.dp))
                    .background(Palette.surface).animateContentSize(Motion.layoutSize).semantics { contentDescription = spoken }) {
                    Column(Modifier.padding(start = 14.dp, end = 14.dp, top = 10.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(tr("Waiting to be sent"), style = Type.caption2, color = Palette.tertiary)
                        Text(entry.quotePreview, style = Type.chatFootnote, color = Palette.text, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    }
                    Row(Modifier.padding(start = 6.dp, end = 6.dp, bottom = 4.dp), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                        QueueAction(tr("Edit"), "pencil") { editing = entry }
                        if (working) QueueAction(tr("Steer it into the reply now"), "arrow.turn.down.right", Palette.accent) { store.steerQueued(entry) }
                        QueueAction(tr("Delete"), "trash") { store.deleteQueued(entry) }
                    }
                    if (entries.size > 1) {
                        Hairline()
                        val rest = entries.size - 1
                        Row(Modifier.fillMaxWidth().clickable(role = Role.Button, onClick = open).padding(horizontal = 14.dp, vertical = 9.dp),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            Symbol("rectangle.stack.fill", size = 12.dp, tint = Palette.secondary)
                            Text(if (rest == 1) tr("1 more waiting") else tr("{count} more waiting").replace("{count}", "$rest"),
                                Modifier.weight(1f), style = Type.caption, color = Palette.secondary)
                            Symbol("chevron.right", size = 10.dp, tint = Palette.tertiary)
                        }
                    }
                }
            }
        }
    }
    EditQueuedDialog(editing, { editing = null }) { entry, text ->
        store.editQueued(entry, text)
        editing = null
    }
}

/** One named action on a waiting message. */
@Composable private fun QueueAction(title: String, systemImage: String, tint: Color = Palette.secondary, action: () -> Unit) {
    val (source, pressed) = rememberPress()
    Row(Modifier.clip(Capsule()).background(Palette.text.copy(alpha = if (pressed) 0.1f else 0f))
        .clickable(source, null, role = Role.Button, onClick = action).padding(horizontal = 10.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
        Symbol(systemImage, size = 12.dp, tint = tint)
        Text(title, style = Type.caption.semibold, color = tint, maxLines = 1)
    }
}

/** The whole queue, oldest first, so a message further back can be changed too. */
@Composable internal fun QueueSheet(entries: List<Entry>, working: Boolean, store: AppStore, close: () -> Unit) {
    var editing by remember { mutableStateOf<Entry?>(null) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(tr("Message queue"))
        if (entries.isEmpty()) {
            Text(tr("Nothing is waiting."), Modifier.padding(horizontal = 20.dp, vertical = 12.dp), style = Type.body, color = Palette.secondary)
        } else {
            Text(tr("Steer sends a message into the reply now; Edit and Delete only change what waits."),
                Modifier.padding(horizontal = 20.dp, vertical = 4.dp), style = Type.caption, color = Palette.secondary)
            Hairline()
            LazyColumn(Modifier.fillMaxWidth().weight(1f)) {
                items(entries, key = { it.id }) { entry ->
                    Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(entry.quotePreview, style = Type.body, color = Palette.text, maxLines = 4, overflow = TextOverflow.Ellipsis)
                        Row(horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                            QueueAction(tr("Edit"), "pencil") { editing = entry }
                            if (working) QueueAction(tr("Steer it into the reply now"), "arrow.turn.down.right", Palette.accent) {
                                store.steerQueued(entry)
                                close()
                            }
                            QueueAction(tr("Delete"), "trash") { store.deleteQueued(entry) }
                        }
                    }
                    Hairline(Modifier.padding(horizontal = 20.dp))
                }
            }
        }
    }
    EditQueuedDialog(editing, { editing = null }) { entry, text ->
        store.editQueued(entry, text)
        editing = null
    }
}

/** Rewrites a waiting message; the agent reads it when its turn comes. */
@Composable private fun EditQueuedDialog(entry: Entry?, dismiss: () -> Unit, save: (Entry, String) -> Unit) {
    var text by remember { mutableStateOf("") }
    // Set when it opens; kept while it fades away.
    LaunchedEffect(entry?.id) { if (entry != null) text = entry.text }
    CodyncDialog("Edit message", entry != null, dismiss, field = {
        val focus = remember { FocusRequester() }
        LaunchedEffect(Unit) { focus.requestFocus() }
        PlainTextField(text, { text = it }, Modifier.fillMaxWidth().focusRequester(focus).pill(Palette.background),
            "Message", style = Type.chatBody, minLines = 2, maxLines = 6)
    }) {
        listOf(DialogAction("Save") {
            val shown = entry
            val rewritten = text.trim()
            if (shown != null && rewritten.isNotEmpty()) save(shown, rewritten)
        })
    }
}

/** The sheet over whichever chat shows the strip or the card's waiting row. */
@Composable internal fun QueueSheetHost(shown: Boolean, entries: List<Entry>, working: Boolean, store: AppStore, close: () -> Unit) {
    CodyncSheet(shown, close, detents = setOf(SheetDetent.Medium, SheetDetent.Expanded)) {
        QueueSheet(entries, working, store, close)
    }
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** While the bot works (anywhere: a room, a routine), the user's waiting messages show above the composer instead of in
 *  the chat (iOS `queue(working:)`), oldest first. A bot that isn't working starts them at once. */
internal fun queuedMessages(chat: List<Entry>, working: Boolean): List<Entry> =
    if (working) chat.filter(Entry::isQueued).sortedBy(Entry::seq) else emptyList()

/** The first host version that takes a queued message now (`steer`) or back (`unqueue`). */
private const val STEER_HOST = "2.39.0"

/** An older host has neither, so its queued messages only show; before its `hello` (offline) the actions stay hidden too. */
internal val AppState.supportsQueueActions: Boolean
    get() = hello?.text("version")?.let { !AppVersion.isBelow(it, STEER_HOST) } == true

/**
 * The user's messages waiting behind the bot's work, as iOS `QueuedMessages`: each one line, sent now into the running
 * turn ([steer]) or taken back ([unqueue]).
 */
@Composable internal fun QueuedMessages(entries: List<Entry>, state: AppState, store: AppStore, modifier: Modifier = Modifier) {
    // The card keeps its last rows while it folds away.
    var shown by remember { mutableStateOf(entries) }
    if (entries.isNotEmpty() && entries != shown) shown = entries
    val actions = state.supportsQueueActions
    val scroll = rememberScrollState()
    val newest = entries.lastOrNull()
    // It stays where it is, the oldest (next to go) in view; a message sent from here shows that it arrived.
    LaunchedEffect(newest?.id) {
        if (newest?.clientNonce != null && newest.clientNonce == store.lastSentNonce) {
            withFrameNanos {} // Its row is laid out by then, so the scroll takes it in.
            scroll.animateScrollTo(scroll.maxValue)
        }
    }
    AnimatedVisibility(entries.isNotEmpty(), modifier, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
        exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
        // A long queue scrolls, so every row stays in reach and the chat keeps its room.
        Column(Modifier.fillMaxWidth().clip(RoundedRectangle(18.dp)).background(Palette.surface).animateContentSize(Motion.layoutSize)
            .heightIn(max = 190.dp).verticalScroll(scroll).padding(vertical = 2.dp)) {
            for (entry in shown) key(entry.id) {
                QueuedRow(entry, actions, steering = "steer:" + entry.id in state.busy, steer = { store.steer(entry) }, remove = { store.unqueue(entry) })
            }
        }
    }
}

@Composable private fun QueuedRow(entry: Entry, actions: Boolean, steering: Boolean, steer: () -> Unit, remove: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(start = 14.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(messagePreview(entry.replyQuote.text), Modifier.weight(1f).padding(vertical = if (actions) 0.dp else 9.dp),
            style = Type.subheadline, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (actions) {
            IconButton("Send now", "arrow.turn.down.right", enabled = !steering, action = steer)
            IconButton("Remove", "trash", enabled = !steering, action = remove)
        }
    }
}
