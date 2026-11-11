package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
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
import com.codync.android.core.ThreadInfo
import com.codync.android.design.*

/**
 * Every thread of a chat, open ones or the archive, with the newest line of each. A thread is a
 * side conversation: opening one puts it over the chat, and long-pressing a row renames, archives
 * or deletes it.
 */
@Composable internal fun ThreadsSheet(bot: Bot, state: AppState, store: AppStore, close: () -> Unit) {
    var archived by remember(bot.id) { mutableStateOf(false) }
    var renaming by remember { mutableStateOf<ThreadInfo?>(null) }
    var draftTitle by remember { mutableStateOf("") }
    var deleting by remember { mutableStateOf<ThreadInfo?>(null) }
    val threads = (if (archived) state.archivedThreads else state.threads)[bot.id].orEmpty()
    LaunchedEffect(bot.id, archived) { store.refreshThreads(bot.id, archived) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(tr("Threads"), trailing = {
            IconButton(tr("New thread"), "plus", enabled = "new-thread:" + bot.id !in state.busy) {
                store.startThread(bot.id)
                close()
            }
        })
        SegmentedChoice(archived, listOf(false to tr("Open"), true to tr("Archived")), { archived = it },
            Modifier.padding(horizontal = 20.dp, vertical = 4.dp))
        Text(tr("A thread keeps one side conversation apart from the chat."), style = Type.caption, color = Palette.secondary,
            modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp))
        Hairline()
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            items(threads, key = { it.id }) { thread ->
                ThreadRow(thread, state, archived,
                    open = { store.openBot(bot.id, thread.id); close() },
                    rename = { renaming = thread; draftTitle = thread.title },
                    archive = { store.archiveThread(thread.id, !archived) },
                    delete = { deleting = thread })
            }
            if (threads.isEmpty() && ("threads:" + bot.id + ":" + archived) !in state.busy) item("empty") {
                Text(
                    if (archived) tr("Nothing archived.") else tr("No threads yet. Open one from the menu, or branch from a message."),
                    style = Type.body, color = Palette.secondary, modifier = Modifier.padding(vertical = 12.dp),
                )
            }
        }
    }
    CodyncDialog(tr("Rename thread"), renaming != null, { renaming = null }, field = {
        PlainInput(draftTitle, { draftTitle = it }, tr("A short name"))
    }) {
        val thread = renaming ?: return@CodyncDialog emptyList()
        listOf(DialogAction("Save") { store.renameThread(thread.id, draftTitle) })
    }
    CodyncDialog(tr("Delete thread?"), deleting != null, { deleting = null },
        message = tr("Its replies go with it; the chat stays.")) {
        val thread = deleting ?: return@CodyncDialog emptyList()
        listOf(DialogAction("Delete thread", destructive = true) { store.deleteThread(thread.id) })
    }
}

@Composable private fun ThreadRow(thread: ThreadInfo, state: AppState, archived: Boolean,
    open: () -> Unit, rename: () -> Unit, archive: () -> Unit, delete: () -> Unit) {
    val lastAuthor = thread.lastAuthor?.let { author ->
        if (author == "user") tr("You") else state.bots.firstOrNull { it.id == author }?.name
    }
    val branch = thread.branchText?.replace('\n', ' ')?.take(140)
    Row(
        Modifier.fillMaxWidth()
            .contextActions(onClick = open, items = {
                buildList {
                    add(MenuItem("Open", icon = "arrow.up.right") { open() })
                    add(MenuItem("Rename", icon = "pencil") { rename() })
                    if (archived) add(MenuItem("Restore", icon = "tray.and.arrow.up") { archive() })
                    else add(MenuItem("Archive", icon = "archivebox") { archive() })
                    add(MenuItem("Delete", icon = "trash", destructive = true, divider = true) { delete() })
                }
            })
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(thread.name, style = Type.body.medium, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (thread.unread > 0) Box(Modifier.size(7.dp).clip(CircleShape).background(Palette.accent))
            }
            val subtitle = listOfNotNull(
                lastAuthor?.let { author -> thread.lastText?.let { "$author: $it" } ?: author },
                branch?.let { tr("Branched from") + " " + it },
            ).firstOrNull()
            if (subtitle != null) Text(subtitle, style = Type.caption, color = Palette.secondary, maxLines = 1,
                overflow = TextOverflow.Ellipsis)
        }
        Text(RelativeTime.short(thread.updatedAt), style = Type.caption2, color = Palette.tertiary)
    }
}
