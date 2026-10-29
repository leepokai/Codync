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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.selection.SelectionContainer
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
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.CallMessage
import com.codync.android.core.Entry
import com.codync.android.core.call
import com.codync.android.core.callConversation
import com.codync.android.core.withCallEntries
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

/** A voice call's line in the chat (iOS `CallLineRow`): its title, written once the call ends, and length; it opens the call. */
@Composable internal fun CallLineRow(entry: Entry, open: () -> Unit) {
    val title = tr(entry.text.ifEmpty { "Voice chat" })
    val length = entry.data["callSeconds"]?.jsonPrimitive?.longOrNull?.let { " · %02d:%02d".format(it / 60, it % 60) }.orEmpty()
    Row(Modifier.fillMaxWidth().plainButton(action = open).semantics(mergeDescendants = true) { contentDescription = title + length }
        .padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally)) {
        Symbol("waveform", size = 13.dp, tint = Palette.secondary)
        Text(title, style = Type.footnote, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(length, style = Type.footnote.monospacedDigit, color = Palette.secondary)
        Symbol("chevron.right", size = 10.dp, tint = Palette.tertiary)
    }
}

/**
 * What a voice call said, oldest first (iOS `CallConversationView`): the user's and the voice's words, and the
 * messages it carried to the bot and back, which the chat itself leaves out.
 */
@Composable internal fun CallConversationView(botId: String, lineId: String, state: AppState, store: AppStore) {
    val entries = state.entries.filter { it.botId == botId }
    val line = entries.firstOrNull { it.id == lineId }
    val call = line?.call
    // The call's own messages, which the chat this phone holds may not reach back to; null while they load.
    var fetched by remember(lineId) { mutableStateOf<List<Entry>?>(null) }
    var error by remember(lineId) { mutableStateOf<String?>(null) }
    LaunchedEffect(botId, call) {
        if (call == null) return@LaunchedEffect
        try { fetched = store.callEntries(botId, call) }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { fetched = emptyList(); error = failure.message ?: "Couldn't load the call." }
    }
    val loading = tr("Loading the call")
    val messages = line?.let { entries.withCallEntries(fetched.orEmpty()).callConversation(it) }.orEmpty()
    val list = rememberLazyListState()
    LaunchedEffect(messages.size) { if (messages.isNotEmpty()) list.scrollToItem(messages.lastIndex) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(tr(line?.text?.ifEmpty { null } ?: "Voice chat"))
        LazyColumn(Modifier.fillMaxSize(), state = list, contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 12.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp)) {
            if (error != null || fetched != null && messages.isEmpty()) item("empty") {
                Text(tr(error ?: "Nothing was said on this call."), Modifier.fillMaxWidth().padding(top = 24.dp), style = Type.footnote,
                    color = if (error != null) Palette.danger else Palette.tertiary, textAlign = TextAlign.Center)
            } else if (fetched == null && call != null) item("loading") {
                Box(Modifier.fillMaxWidth().padding(top = 24.dp).clearAndSetSemantics { contentDescription = loading },
                    contentAlignment = Alignment.Center) { Spinner(14.dp) }
            }
            itemsIndexed(messages) { _, message -> CallMessageRow(message) }
        }
    }
}

@Composable private fun CallMessageRow(message: CallMessage) {
    when (message.speaker) {
        CallMessage.Speaker.User -> Box(Modifier.fillMaxWidth().padding(start = 40.dp), contentAlignment = Alignment.CenterEnd) {
            SelectionContainer {
                Text(message.text, Modifier.clip(RoundedRectangle(18.dp)).background(Palette.bubbleUser).padding(horizontal = 14.dp, vertical = 9.dp),
                    style = Type.body, color = Palette.text)
            }
        }
        CallMessage.Speaker.Voice -> SelectionContainer {
            Text(message.text, Modifier.fillMaxWidth().padding(end = 40.dp), style = Type.body, color = Palette.secondary)
        }
        CallMessage.Speaker.VoiceToBot -> SelectionContainer { Text("↳ " + message.text, style = Type.footnote, color = Palette.secondary) }
        CallMessage.Speaker.Bot -> Box(Modifier.fillMaxWidth().padding(end = 40.dp), contentAlignment = Alignment.CenterStart) {
            Box(Modifier.clip(RoundedRectangle(22.dp)).background(Palette.bubbleAgent).padding(horizontal = 16.dp, vertical = 10.dp)) {
                MarkdownText(message.text)
            }
        }
    }
}
