package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.design.*

/** How fast messages read aloud, shared with voice conversations. */
internal val readingSpeeds = listOf(0.75f, 1f, 1.25f, 1.5f, 2f)

internal fun speedLabel(rate: Float): String =
    (if (rate == rate.toInt().toFloat()) rate.toInt().toString() else rate.toString().trimEnd('0')) + "×"

/** Picks the speed a message reads at; it stays for the next ones and for calls. */
@Composable internal fun ReadingSpeedSheet(store: AppStore, close: () -> Unit) {
    val current = store.readingSpeed()
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(tr("Reading speed"))
        Text(tr("Read aloud speed, also used in voice conversations."), style = Type.caption, color = Palette.secondary,
            modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp))
        Hairline()
        Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            for (rate in readingSpeeds) {
                Row(Modifier.fillMaxWidth().clickable(role = androidx.compose.ui.semantics.Role.Button) {
                    store.setReadingSpeed(rate)
                    close()
                }.padding(vertical = 10.dp), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(speedLabel(rate), Modifier.weight(1f), style = Type.body, color = Palette.text)
                    if (rate == current) Symbol("checkmark", size = 16.dp, tint = Palette.accent)
                }
            }
        }
    }
}
