package com.codync.android

import androidx.compose.runtime.Composable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.ui.unit.dp
import com.codync.android.core.LinkState
import com.codync.android.design.*

/** The first things this computer needs, and what is missing; it disappears once all four are done. */
@Composable internal fun GettingStartedSection(state: AppState) {
    val agents = backends(state)
    val agent = agents.any { it.text("available") == "true" || it.text("installed") == "true" }
    val signedIn = agents.any { it.text("signedIn") == "true" }
    val steps = listOf(
        Triple("This computer", state.actualConnection is LinkState.Ready, if (state.actualConnection is LinkState.Ready) "Answering" else "Not running"),
        Triple("A coding agent", agent && signedIn, if (agent) (if (signedIn) "Installed and signed in" else "Installed; sign in") else "None installed"),
        Triple("A bot", state.bots.isNotEmpty(), if (state.bots.isEmpty()) "None yet" else "${state.bots.size} ready"),
        Triple("Computer access", false, "Grant it to let bots use apps and the screen"),
    )
    if (steps.none { !it.second }) return
    CardSection("Getting started", footer = "Each step is set on this computer, in Settings or Extensions.") {
        for ((title, done, note) in steps) {
            row {
                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(title, style = Type.body, color = Palette.text)
                    Text(note, style = Type.caption, color = if (done) Palette.secondary else Palette.tertiary)
                }
            }
        }
    }
}
