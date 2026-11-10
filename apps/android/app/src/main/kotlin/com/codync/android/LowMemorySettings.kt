package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.codync.android.design.*

/** Settings: keep less chat history in memory on this phone, with an info button saying when that helps. */
@Composable
internal fun LowMemorySettings(store: AccountStore) {
    val on by LowMemoryMode.enabled.collectAsStateWithLifecycle()
    var about by remember { mutableStateOf(false) }
    val label = tr("Low memory mode")
    CardSection("Memory", footer = "Only changes this device. Older messages stay on the phone and load as you scroll back.") {
        row {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(label, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                IconButton("About low memory mode", "info.circle") { about = true }
                Box(Modifier.weight(1f))
                CodyncSwitch(on, { value -> LowMemoryMode.set(value); store.stores.value.forEach { it.reloadMirror() } },
                    Modifier.width(34.dp).semantics { contentDescription = label }) {}
            }
        }
    }
    CodyncDialog("Low memory mode", about, { about = false },
        "Turn this on if Codync closes by itself, most often with many bots or very long chats. It keeps less of each chat in memory. Older messages are still there and load as you scroll back.", cancel = "Got it") {
        emptyList()
    }
}
