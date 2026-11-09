package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** A thread: the message it started on, its replies, and a box to continue there. */
@Composable internal fun RepliesView(bot: Bot, rootId: String, state: AppState, store: AppStore, connect: ConnectionRequestActions,
    documentTitle: String? = null,
    openTrace: () -> Unit = { store.showTrace(true) }) {
    val root = state.entries.firstOrNull { it.botId == bot.id && it.id == rootId }
    val replies = state.entries.filter { it.botId == bot.id && it.threadId == rootId && it.isChat }
    val pending = state.pending.filter { it.botId == bot.id && it.threadId == rootId }
    LaunchedEffect(rootId, replies.lastOrNull()?.rev) {
        if (rootId.startsWith("doc:")) store.acknowledgeVisible(listOf(ReadingScope(bot.id, rootId)))
    }
    val working = bot.isWorking(bot.id) && state.connection is LinkState.Ready
    var composerHeight by remember { mutableIntStateOf(0) }
    val scroll = rememberConversationScroll()
    val density = LocalDensity.current
    val dismiss = LocalDismissModal.current
    val info = state.threadInfo
    val doc = rootId.startsWith("doc:")
    var renaming by remember { mutableStateOf(false) }
    var renameText by remember { mutableStateOf("") }
    var confirmingDelete by remember { mutableStateOf(false) }
    // Jump to latest floats as chrome, frosting the replies scrolling under it.
    GlassStage(Modifier.fillMaxSize(), chrome = {
        JumpToLatest(scroll, Modifier.align(Alignment.BottomCenter).padding(bottom = with(density) { composerHeight.toDp() } + 10.dp))
    }) {
        Column(Modifier.fillMaxSize()) {
            Row(Modifier.fillMaxWidth().height(56.dp).padding(start = 20.dp, end = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(if (doc) tr("Document conversation") else info?.name ?: tr("Thread"), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(if (doc) listOfNotNull(documentTitle, bot.name).joinToString(" · ") else bot.name, style = Type.caption, color = Palette.tertiary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                if (!doc) DropdownMenu({
                    buildList {
                        add(MenuItem("Rename thread", icon = "pencil") { renameText = info?.title.orEmpty(); renaming = true })
                        if (info?.archived == true) add(MenuItem("Restore", icon = "tray.and.arrow.up") { store.archiveThread(rootId, false) })
                        else add(MenuItem("Archive thread", icon = "archivebox") { store.archiveThread(rootId, true) })
                        add(MenuItem("Delete thread", icon = "trash", destructive = true, divider = true) { confirmingDelete = true })
                    }
                }, Modifier.size(36.dp), label = tr("Thread actions")) { Symbol("ellipsis", size = 21.dp, tint = Palette.secondary) }
                IconButton(if (doc) "Close document conversation" else "Close thread", "xmark", action = dismiss)
            }
            Box(Modifier.fillMaxWidth().height(0.5.dp).background(Palette.border))
            val items = chatItems(replies, pending)
            // The replies unread when the thread opened; reading it clears them, so keep the count for this visit.
            val unread = remember(rootId) {
                (root?.data?.get("thread") as? kotlinx.serialization.json.JsonObject)
                    ?.get("unread")?.let { (it as? kotlinx.serialization.json.JsonPrimitive)?.content?.toIntOrNull() } ?: 0
            }
            val rows = remember(items, unread) {
                if (unread <= 0) items else {
                    val indexes = items.withIndex().filter { it.value is ChatItem.Message }.map { it.index }
                    val cut = indexes.getOrNull(indexes.size - unread) ?: return@remember items
                    items.toMutableList().apply { add(cut, ChatItem.NewLine("new-$rootId")) }
                }
            }
            Box(Modifier.weight(1f)) { ConversationList(scroll, replies.map { it.id }, items.lastOrNull()?.id, replies.lastOrNull()?.rev, working,
                PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = with(density) { composerHeight.toDp() } + 8.dp)) {
                if (root != null) {
                    val branch = info?.branchText
                    if (!doc && branch != null) item("branch") { BranchLine(branch) { store.openBot(bot.id) } }
                    item("root") { ChatRow(root, true, bot, state, store, openTrace = openTrace, reply = null, connect = connect, thread = rootId) }
                    item("count") {
                        val count = (root.data["thread"] as? kotlinx.serialization.json.JsonObject)?.get("count")
                            ?.let { (it as? kotlinx.serialization.json.JsonPrimitive)?.content?.toIntOrNull() } ?: 0
                        Row(Modifier.padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text(if (count == 0) tr("No replies yet") else if (count == 1) tr("1 reply") else "$count " + tr("replies"), style = Type.caption, color = Palette.tertiary)
                            Box(Modifier.weight(1f).height(1.dp).background(Palette.border))
                        }
                    }
                }
                if (rootId.startsWith("doc:") && items.isEmpty() && !working) item("document-intro") {
                    Column(Modifier.padding(vertical = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Text(if ("thread:$rootId" in state.busy) "Loading document conversation…" else
                            "Ask this bot about the saved document. This page has its own conversation.", style = Type.body, color = Palette.secondary)
                        state.error?.let { Text(it, style = Type.body, color = Palette.danger) }
                        if (state.error != null) LinkButton("Retry loading conversation") { store.loadDocumentConversation(bot.id, rootId) }
                    }
                }
                items(rows.filter { it !is ChatItem.Separator }, key = { it.id }) { item ->
                    when (item) {
                        is ChatItem.Message -> ChatRow(item.entry, item.groupStart, bot, state, store, openTrace = openTrace, reply = null, connect = connect, thread = rootId)
                        is ChatItem.Pending -> PendingBubble(item.send, item.groupStart, state, store)
                        is ChatItem.NewLine -> NewLineRow(null)
                        is ChatItem.Separator, is ChatItem.Exchanges -> {}
                    }
                }
                if (working) item("working") { WorkingIndicator(bot, currentThinking(state.entries, bot.id, rootId)) }
            } }
        }
        Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth().onSizeChanged { composerHeight = it.height }) {
            val mismatch = state.mismatch
            if (mismatch != null) UpdateNeededCard(state, mismatch, store::requestHostUpdate, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp))
            else MessageComposer(bot, rootId, state, store, onCall = null)
        }
    }
    if (!doc) {
        CodyncDialog(tr("Rename thread"), renaming, { renaming = false }, field = {
            PlainInput(renameText, { renameText = it }, tr("A short name"))
        }) {
            listOf(DialogAction("Save") { store.renameThread(rootId, renameText) })
        }
        CodyncDialog(tr("Delete thread?"), confirmingDelete, { confirmingDelete = false },
            message = tr("Its replies go with it; the chat stays.")) {
            listOf(DialogAction("Delete thread", destructive = true) { store.deleteThread(rootId) })
        }
    }
}

/** The message a thread was branched from, and a jump to it in the chat. */
@Composable private fun BranchLine(text: String, open: () -> Unit) {
    val branched = tr("Branched from")
    Row(
        Modifier.fillMaxWidth().padding(top = 6.dp).clip(RoundedRectangle(12.dp)).background(Palette.surface)
            .clickable(role = Role.Button, onClick = open).padding(horizontal = 12.dp, vertical = 8.dp)
            .semantics { contentDescription = "$branched $text" },
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Symbol("arrow.turn.down.right", size = 12.dp, tint = Palette.tertiary)
        Column(Modifier.weight(1f)) {
            Text(branched, style = Type.caption2.medium, color = Palette.tertiary)
            Text(text, style = Type.caption, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}
