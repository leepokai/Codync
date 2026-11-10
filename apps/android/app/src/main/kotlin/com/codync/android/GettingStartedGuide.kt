package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.LinkState
import com.codync.android.design.*

/** The first-run guide after pairing: what is ready and what is missing, dismissed once. */
@Composable internal fun GettingStartedGuide(state: AppState, done: () -> Unit) {
    val agents = backends(state)
    val agent = agents.any { it.text("available") == "true" || it.text("installed") == "true" }
    val signedIn = agents.any { it.text("signedIn") == "true" }
    val steps = listOf(
        Triple("This computer", state.actualConnection is LinkState.Ready, if (state.actualConnection is LinkState.Ready) "Answering" else "Not running"),
        Triple("A coding agent", agent && signedIn, if (agent) (if (signedIn) "Installed and signed in" else "Installed; sign in in Extensions") else "None installed; add one in Extensions"),
        Triple("A bot", state.bots.isNotEmpty(), if (state.bots.isEmpty()) "None yet; create one from Bots" else "${state.bots.size} ready"),
        Triple("Computer access", false, "Grant it on the computer if bots should use apps and the screen"),
    )
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        ModalHeader("Getting started")
        Text("Each step is set on this computer; the same pages stay in Settings and Extensions.", style = Type.caption, color = Palette.secondary)
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            for ((title, ok, note) in steps) {
                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(title, style = Type.body, color = Palette.text)
                    Text(note, style = Type.caption, color = if (ok) Palette.secondary else Palette.tertiary)
                }
            }
        }
        PrimaryButton("Done") { done() }
        SecondaryButton("Not now") { done() }
    }
}
