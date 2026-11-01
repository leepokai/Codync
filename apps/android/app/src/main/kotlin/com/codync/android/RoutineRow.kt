package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.codync.android.core.Routine
import com.codync.android.design.*

/** One routine in a list: its name and a line under it (opens the editor), and its on/off switch (iOS `RoutineRow`). */
@Composable internal fun RoutineRow(routine: Routine, summary: String, enabled: Boolean, open: () -> Unit, toggle: (Boolean) -> Unit) {
    Row(Modifier.outlinedCard(vertical = 0.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Column(Modifier.weight(1f).plainButton(action = open).padding(vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(routine.name, style = Type.system(13f), color = Palette.text)
            Text(tr(summary), style = Type.caption, color = if (routine.lastError == null) Palette.secondary else Palette.danger)
        }
        CodyncSwitch(routine.enabled, toggle, Modifier.width(34.dp).semantics { contentDescription = routine.name }, enabled = enabled) {}
    }
}
