package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.design.*
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

private val RUN_WORDS = mapOf(
    "pending" to "Waiting its turn",
    "starting" to "Starting",
    "running" to "Running",
    "recovering" to "Resuming after a restart",
    "succeeded" to "Done",
    "failed" to "Failed",
    "interrupted" to "Interrupted",
    "cancelled" to "Cancelled",
)

private fun threadName(id: String): String = when {
    id.startsWith("doc-group:") -> "Group document"
    id.startsWith("doc:") -> "Document"
    id.startsWith("slack-") -> "Slack thread"
    id.startsWith("routine") -> "Routine thread"
    else -> id
}

/** What a bot waits on, derived by the host: approvals, routine runs and unread threads. */
@Composable internal fun BotTasksView(bot: Bot, state: AppState, store: AppStore) {
    var data by remember(bot.id) { mutableStateOf<JsonObject?>(null) }
    var error by remember(bot.id) { mutableStateOf<String?>(null) }
    LaunchedEffect(bot.id, state.generation) {
        try { data = store.botTasks(bot.id) } catch (e: Exception) { error = e.message }
    }
    val approvals: JsonArray = data?.get("approvals")?.jsonArray ?: JsonArray(emptyList())
    val runs: JsonArray = data?.get("runs")?.jsonArray ?: JsonArray(emptyList())
    val threads: JsonArray = data?.get("threads")?.jsonArray ?: JsonArray(emptyList())
    CardForm {
        CardSection("Approvals") {
            if (approvals.isEmpty()) row { Text("Nothing waiting for you.", style = Type.body, color = Palette.secondary) }
            for (approval in approvals) {
                val item = approval.jsonObject
                row {
                    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(item["title"]?.jsonPrimitive?.content ?: "Approval", style = Type.body, color = Palette.text)
                        item["detail"]?.jsonPrimitive?.content?.takeIf(String::isNotBlank)?.let {
                            Text(it, style = Type.caption, color = Palette.tertiary, maxLines = 3)
                        }
                    }
                }
            }
        }
        CardSection("Routine runs") {
            if (runs.isEmpty()) row { Text("No routine runs yet.", style = Type.body, color = Palette.secondary) }
            for (run in runs) {
                val item = run.jsonObject
                val status = item["status"]?.jsonPrimitive?.content.orEmpty()
                row {
                    ValueRow(tr(RUN_WORDS[status] ?: status), if (item["threadRoot"]?.jsonPrimitive?.content != null) "In a thread" else "In the chat")
                }
            }
        }
        CardSection("Threads") {
            if (threads.isEmpty()) row { Text("Every thread is read.", style = Type.body, color = Palette.secondary) }
            for (thread in threads) {
                val item = thread.jsonObject
                val id = item["id"]?.jsonPrimitive?.content.orEmpty()
                row { ValueRow(tr(threadName(id)), item["unread"]?.jsonPrimitive?.content.orEmpty()) }
            }
        }
        error?.let { message ->
            CardSection("Status") { row { Text(message, style = Type.caption, color = Palette.danger) } }
        }
    }
}
