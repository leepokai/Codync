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
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import java.io.IOException

/** While the bot works (anywhere: a room, a routine), the user's waiting messages show above the composer instead of in
 *  the chat (iOS `queue(working:)`), oldest first. A bot that isn't working starts them at once. */
internal fun queuedMessages(chat: List<Entry>, working: Boolean): List<Entry> =
    if (working) chat.filter(Entry::isQueued).sortedBy(Entry::seq) else emptyList()

/**
 * A queued stored stub opens the same sheet a transcript placeholder opens ([LocalShowStoredMessage]).
 * A short queued message already shows its text, and a stub with no row to read does not offer an open.
 */
internal fun Entry.opensQueuedStoredMessage(): Boolean =
    isQueued && isStoredStub && id.isNotBlank() && botId.isNotBlank()

/**
 * The text that open yields, from the stored row's [payload] via the sheet's own read ([storedTextOfPayload]).
 * Null when the row must not open, or the payload is missing, unreadable, or blank: the row then stays the line it is.
 */
internal fun queuedStoredText(entry: Entry, payload: String?): String? {
    if (!entry.opensQueuedStoredMessage() || payload == null) return null
    return try {
        storedTextOfPayload(payload, entry.id, complete = true)
    } catch (_: IOException) {
        null
    }
}

/** The first host version that takes a queued message now (`steer`) or back (`unqueue`). */
private const val STEER_HOST = "2.39.0"

/** An older host has neither, so its queued messages only show; before its `hello` (offline) the actions stay hidden too. */
internal val AppState.supportsQueueActions: Boolean
    get() = hello?.text("version")?.let { !AppVersion.isBelow(it, STEER_HOST) } == true

/**
 * The user's messages waiting behind the bot's work, as iOS `QueuedMessages`: each one line, sent now into the running
 * turn ([steer]) or taken back ([remove]): the chat's own unless said (a thread's go by its task).
 */
@Composable internal fun QueuedMessages(entries: List<Entry>, state: AppState, store: AppStore, modifier: Modifier = Modifier,
    steer: (Entry) -> Unit = { store.steer(it) }, remove: (Entry) -> Unit = { store.unqueue(it) }) {
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
                QueuedRow(entry, actions, steering = "steer:" + entry.id in state.busy, steer = { steer(entry) }, remove = { remove(entry) })
            }
        }
    }
}

@Composable private fun QueuedRow(entry: Entry, actions: Boolean, steering: Boolean, steer: () -> Unit, remove: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(start = 14.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp)) {
        val files = entry.attachments
        // Its files show as a symbol, and by name when it has no words.
        if (files.isNotEmpty()) Symbol(if (files.all { it.isImage }) "photo" else "doc", size = 13.dp, tint = Palette.secondary,
            modifier = Modifier.padding(end = 4.dp))
        val quoted = entry.replyQuote.text
        val text = (if (entry.isStoredStub && quoted == STORED_STUB_ID) entry.stubLabel() else quoted).ifEmpty { files.joinToString(", ") { it.name } }
        // The placeholder line opens the stored sheet. Send now and Remove stay their own buttons.
        val openStored = LocalShowStoredMessage.current?.takeIf { entry.opensQueuedStoredMessage() }
        val preview = Modifier.weight(1f).padding(vertical = if (actions) 0.dp else 9.dp)
        val line = openStored?.let { open ->
            preview.clickable(role = Role.Button, onClickLabel = tr("Show full message")) { open(entry) }
        } ?: preview
        Text(messagePreview(text), line, style = Type.subheadline, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (actions) {
            IconButton("Send now", "arrow.turn.down.right", enabled = !steering, action = steer)
            IconButton("Remove", "trash", enabled = !steering, action = remove)
        }
    }
}
