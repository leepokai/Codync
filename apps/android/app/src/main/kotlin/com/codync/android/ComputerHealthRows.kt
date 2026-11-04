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
import com.codync.android.core.HostHealth
import com.codync.android.design.*
import kotlinx.coroutines.delay
import kotlinx.serialization.json.buildJsonObject

/** The computer's health as the host reports it; null from an older host or when it cannot be reached. */
suspend fun AppStore.hostHealth(): HostHealth? = runCatching { HostHealth.decode(hostQuery("hostHealth", buildJsonObject { })) }.getOrNull()

/** The status tab's health row: memory, CPU, disk and what each agent uses, read again every few seconds. */
internal fun CardRows.healthRows(store: AppStore, key: Any?) {
    row { ComputerHealth(store, key) }
}

@Composable private fun ComputerHealth(store: AppStore, key: Any?) {
    var health by remember(key) { mutableStateOf<HostHealth?>(null) }
    LaunchedEffect(key) {
        while (true) {
            health = store.hostHealth() ?: health
            delay(10_000)
        }
    }
    val h = health ?: return
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(tr(HostHealth.levelLabel(h.level)), style = Type.body, color = if (h.level != "normal") Palette.warning else Palette.text)
        for ((label, value) in h.lines()) ValueRow(tr(label), value)
        for ((name, use) in h.agentRows()) ValueRow(name, use, detail = tr("Agent"))
    }
}
