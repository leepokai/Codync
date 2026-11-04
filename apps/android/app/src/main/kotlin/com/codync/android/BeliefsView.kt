package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.dp
import com.codync.android.core.Belief
import com.codync.android.design.*
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** The bot's notes as beliefs; null from an older host. */
suspend fun AppStore.beliefs(botId: String, forgotten: Boolean): List<Belief>? = runCatching {
    Belief.decodeAll(hostQuery("beliefs", buildJsonObject { put("botId", botId); put("forgotten", forgotten) }))
}.getOrNull()

/** Confirms a note, forgets it, or brings an earlier version back. */
suspend fun AppStore.beliefAction(botId: String, belief: Belief, action: String, versionId: Long? = null): JsonElement =
    hostQuery("beliefAction", buildJsonObject {
        put("botId", botId); put("file", belief.file); put("shared", belief.shared); put("action", action)
        versionId?.let { put("versionId", it) }
    })

/** Takes back what the bot just learned (the "I learned" line's Undo). */
suspend fun AppStore.undoMemoryChange(botId: String, entryId: String): JsonElement =
    hostQuery("undoMemoryChange", body("botId" to botId, "entryId" to entryId))

/** "What I know about you": the notes as beliefs with how sure the bot is, filters, and confirm, forget and bring back. */
internal fun CardRows.beliefsRows(botId: String, store: AppStore, rev: Long?) {
    row { BeliefsPanel(botId, store, rev) }
}

@Composable private fun BeliefsPanel(botId: String, store: AppStore, rev: Long?) {
    var beliefs by remember(botId) { mutableStateOf<List<Belief>?>(null) }
    var kind by remember { mutableStateOf<String?>(null) }
    var confidence by remember { mutableStateOf<String?>(null) }
    var forgotten by remember { mutableStateOf(false) }
    var open by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    suspend fun load() { beliefs = store.beliefs(botId, forgotten) }
    LaunchedEffect(botId, forgotten, rev) { load() }
    fun act(belief: Belief, action: String, versionId: Long? = null) {
        scope.launch {
            error = null
            try { store.beliefAction(botId, belief, action, versionId); load() } catch (e: Exception) { error = e.message ?: "That did not work." }
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(tr(if (forgotten) "Forgotten notes stay in the history. Bring one back if you change your mind."
            else "What this bot believes about you, the least sure first. Confirm what holds; tell the bot what to correct."),
            style = Type.footnote, color = Palette.secondary)
        ValueRow("Kind") { ChoicePicker(kind.orEmpty(), listOf("" to "All") + Belief.KINDS, { kind = it.ifEmpty { null } }, label = "Kind") }
        ValueRow("Confidence") { ChoicePicker(confidence.orEmpty(), listOf("" to "Any") + Belief.CONFIDENCES, { confidence = it.ifEmpty { null } }, label = "Confidence") }
        SecondaryButton(if (forgotten) "Back to what I know" else "Show forgotten") { forgotten = !forgotten; beliefs = null }
        error?.let { Text(it, style = Type.caption, color = Palette.danger) }
        val list = beliefs
        if (list == null) Text(tr("Loading…"), style = Type.body, color = Palette.secondary)
        else {
            val shown = Belief.byAttention(Belief.filter(list, kind, confidence))
            if (shown.isEmpty()) Text(tr(if (forgotten) "Nothing forgotten." else "Nothing here yet."), style = Type.body, color = Palette.secondary)
            for (belief in shown) Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                val id = "${belief.shared}:${belief.file}"
                Text(belief.description ?: belief.file, style = Type.body, color = Palette.text, maxLines = 2)
                Text("${tr(Belief.kindLabel(belief.kind))} · ${tr(Belief.confidenceLabel(belief.confidence))} · ${tr(Belief.confirmedText(belief.confirmed))}" +
                    (if (belief.shared) " · ${tr("Shared")}" else ""), style = Type.caption, color = Palette.secondary)
                if (!forgotten) {
                    SecondaryButton("Still true") { act(belief, "confirm") }
                    SecondaryButton("Forget") { act(belief, "forget") }
                }
                SecondaryButton(if (open == id) "Hide history" else "History") { open = if (open == id) null else id }
                if (open == id) for (row in belief.historyRows()) {
                    ValueRow("${tr(row.label)} · ${ago(row.at)}") {
                        if (row.restorable) SecondaryButton("Bring back") { act(belief, "restore", row.id) }
                    }
                }
            }
        }
    }
}

/** The "I learned …" line of a bot's chat: what it learned, with an Undo until it is taken back. */
@Composable internal fun LearnedNotice(entry: com.codync.android.core.Entry, store: AppStore) {
    val undo = entry.data["memoryUndo"] as? kotlinx.serialization.json.JsonObject
    val undone = (undo?.get("undone") as? kotlinx.serialization.json.JsonPrimitive)?.content == "true"
    val scope = rememberCoroutineScope()
    var busy by remember(entry.id) { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(entry.text, style = Type.chatFootnote, color = Palette.secondary)
        if (!undone) SecondaryButton("Undo", enabled = !busy) {
            busy = true
            scope.launch { try { store.undoMemoryChange(entry.botId, entry.id) } finally { busy = false } }
        }
    }
}
