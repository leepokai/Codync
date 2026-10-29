package com.codync.android

import android.content.ClipData
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.Bot
import com.codync.android.core.BotConversationRow
import com.codync.android.core.BotExchange
import com.codync.android.core.BotExchangeGroup
import com.codync.android.core.Entry
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** A bot's name; [fallback] for one that isn't this computer's (a bot on another computer). */
internal fun AppState.authorName(id: String?, fallback: String? = null): String =
    bots.firstOrNull { it.id == id }?.name ?: fallback ?: "A deleted bot"

/** The compact line a bot-to-bot exchange leaves in the chat; tapping it opens the conversation (iOS `BotMessageRow`). */
@Composable internal fun BotMessageRow(group: BotExchangeGroup, state: AppState, open: () -> Unit) {
    val peer = state.bots.firstOrNull { it.id == group.peerId }
    val name = state.authorName(group.peerId, group.peerName)
    val ink = if (group.failed) Palette.danger else Palette.secondary
    Row(Modifier.fillMaxWidth().padding(top = 2.dp).plainButton(action = open)
        .semantics(mergeDescendants = true) { contentDescription = "${group.title} $name${if (group.failed) ", failed" else ""}" }
        .padding(vertical = 10.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally)) {
        if (group.failed) Symbol("exclamationmark.triangle.fill", size = 13.dp, tint = Palette.danger)
        Text(group.title, style = Type.footnote, color = ink)
        if (peer != null) BotAvatar(peer, state.bots, 18.dp)
        Text(name, style = Type.footnote.semibold, color = if (group.failed) Palette.danger else Palette.text)
        Symbol("chevron.right", size = 10.dp, tint = Palette.tertiary)
    }
}

/** Read-only history between a bot and one peer, newest at the bottom (iOS `BotConversationView`). */
@Composable internal fun BotConversationView(bot: Bot, peerId: String, state: AppState, store: AppStore) {
    var fetched by remember(peerId) { mutableStateOf<List<Entry>?>(null) }
    var error by remember(peerId) { mutableStateOf<String?>(null) }
    LaunchedEffect(bot.id, peerId) {
        try {
            fetched = entriesOf(store.hostCall("botConversation", buildJsonObject { put("botId", bot.id); put("peerId", peerId) }, 20_000))
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { fetched = emptyList(); error = failure.message ?: "Couldn't load the conversation." }
    }
    val exchanges = BotExchange.conversation(fetched.orEmpty(), state.entries.filter { it.botId == bot.id }, peerId)
    val rows = BotConversationRow.build(exchanges)
    // A peer on another computer goes by the name its exchanges carry.
    val remoteName = exchanges.firstNotNullOfOrNull { it.peerName }
    val name = { id: String -> state.authorName(id, remoteName.takeIf { id == peerId }) }
    val list = rememberLazyListState()
    LaunchedEffect(rows.size) { if (rows.isNotEmpty()) list.scrollToItem(rows.lastIndex) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader({ PairTitle(bot.id, peerId, state, name) })
        LazyColumn(Modifier.fillMaxSize(), state = list, contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 12.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp)) {
            if (error != null || rows.isEmpty() && fetched != null) item("empty") {
                Text(tr(error ?: "No messages yet."), Modifier.fillMaxWidth().padding(top = 24.dp), style = Type.footnote,
                    color = if (error != null) Palette.danger else Palette.tertiary, textAlign = TextAlign.Center)
            }
            items(rows, key = { it.id }) { row ->
                when (row) {
                    is BotConversationRow.Separator -> TimeSeparator(row.at)
                    is BotConversationRow.Message -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        if (row.showsAuthor) Box(Modifier.padding(top = 8.dp)) { AuthorLabel(row.author, state, name(row.author)) }
                        ConversationBubble(row.text)
                    }
                    is BotConversationRow.Status -> when (val outcome = row.outcome) {
                        BotExchange.Outcome.Pending -> Text("Waiting for ${name(row.awaiting)}…", Modifier.padding(start = 12.dp),
                            style = Type.footnote, color = Palette.secondary)
                        is BotExchange.Outcome.Failed -> SelectionContainer {
                            Text(outcome.detail.ifEmpty { "Didn't complete." }, Modifier.padding(start = 12.dp), style = Type.footnote, color = Palette.danger)
                        }
                        BotExchange.Outcome.Done -> {}
                    }
                }
            }
        }
    }
}

/** "[avatar] A ⇄ [avatar] B": the two bots of a conversation, the chat's own bot first. */
@Composable private fun PairTitle(botId: String, peerId: String, state: AppState, name: (String) -> String) {
    Row(Modifier.semantics(mergeDescendants = true) { contentDescription = "${name(botId)} and ${name(peerId)}" },
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for ((index, id) in listOf(botId, peerId).withIndex()) {
            if (index > 0) Symbol("arrow.left.arrow.right", size = 13.dp, tint = Palette.tertiary)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                state.bots.firstOrNull { it.id == id }?.let { BotAvatar(it, state.bots, 22.dp) }
                Text(name(id), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/** A message of the conversation: the agent bubble, copyable. */
@Composable private fun ConversationBubble(text: String) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    Box(Modifier.fillMaxWidth().padding(end = 40.dp), contentAlignment = Alignment.CenterStart) {
        Box(Modifier.clip(RoundedRectangle(22.dp)).background(Palette.bubbleAgent).contextActions {
            listOf(MenuItem("Copy", icon = "square.on.square") {
                scope.launch { clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("Message", text))) }
            })
        }.padding(horizontal = 16.dp, vertical = 10.dp)) { MarkdownText(text) }
    }
}
