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
}
