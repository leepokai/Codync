package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.design.*

/** Picks the chat a message is sent on to ("Send to a channel"). */
@Composable internal fun ForwardSheet(entry: Entry, state: AppState, store: AppStore, close: () -> Unit) {
    val bots = state.bots.sortedBy { it.name }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(tr("Send to a channel"))
        Text(tr("The message arrives there as a forwarded copy."), style = Type.caption, color = Palette.secondary,
            modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp))
        Hairline()
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp)) {
            items(bots, key = { it.id }) { bot ->
                Row(Modifier.fillMaxWidth().clickable(enabled = "forward:" + entry.id !in state.busy) {
                    store.forwardEntry(entry, bot.id) { close() }
                }.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    BotAvatar(bot, state.bots, 32.dp)
                    Column(Modifier.weight(1f)) {
                        Text(bot.name, style = Type.body, color = Palette.text, maxLines = 1,
                            overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                        if (bot.kind == "group") Text(tr("Group chat"), style = Type.caption, color = Palette.secondary)
                    }
                    if ("forward:" + entry.id in state.busy) Spinner(size = 14.dp)
                }
            }
            if (bots.isEmpty()) item { Text(tr("No chats yet."), style = Type.body, color = Palette.secondary) }
        }
    }
}
