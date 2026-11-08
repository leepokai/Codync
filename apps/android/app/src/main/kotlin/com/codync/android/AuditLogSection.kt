package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import kotlinx.coroutines.launch
import java.text.DateFormat
import java.util.Date

/** The security decisions made on the computer, newest first; kept for 90 days. */
@Composable internal fun AuditLogSection(state: AppState, store: AppStore) {
    val scope = rememberCoroutineScope()
    var lines by remember { mutableStateOf<List<Pair<Long, Pair<String, String>>>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    fun read() {
        scope.launch {
            try { lines = store.auditLog(); error = null } catch (e: Exception) { error = e.message }
        }
    }
    LaunchedEffect(state.contextId) { read() }
    CardSection("Audit log", "Pairing, revoked access, cloud, network and approval decisions. Nothing from chats; kept for 90 days.") {
        row {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SecondaryButton("Refresh") { read() }
                SecondaryButton("Clear log", enabled = !lines.isNullOrEmpty()) {
                    scope.launch { try { store.clearAuditLog(); lines = emptyList() } catch (e: Exception) { error = e.message } }
                }
            }
        }
        val shown = lines
        row {
            when {
                error != null -> Text(error!!, style = Type.caption, color = Palette.danger)
                shown == null -> Text("Loading…", style = Type.caption, color = Palette.secondary)
                shown.isEmpty() -> Text("Nothing yet.", style = Type.caption, color = Palette.secondary)
                else -> Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    for ((at, pair) in shown.take(50)) {
                        Column(Modifier.fillMaxWidth()) {
                            Text(tr(pair.first), style = Type.caption, color = Palette.text)
                            if (pair.second.isNotBlank()) Text(pair.second, style = Type.caption, color = Palette.tertiary)
                            Text(DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(at)),
                                style = Type.caption, color = Palette.tertiary)
                        }
                    }
                }
            }
        }
    }
}
