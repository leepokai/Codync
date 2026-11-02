package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.codync.android.core.CloudMachine
import com.codync.android.design.*

/** "Codync Cloud · Boat · EU": a cloud computer's line in the computer list (the brand stays English). */
internal val CloudMachine.summary: String get() = "Codync Cloud · $providerName · $region"

private val CloudMachine.stateLabel: String get() = when (state) {
    "starting" -> "Starting"
    "running" -> "Running"
    "stopping" -> "Going to sleep"
    "stopped" -> "Sleeping"
    "failed" -> "Failed"
    else -> state
}

/**
 * Computers → Codync Cloud: the computer Codync runs for this account. Shown only to an account that has one or may
 * create one (the feature isn't public yet).
 */
@Composable internal fun CloudSettings(state: AppState, store: AppStore) {
    LaunchedEffect(state.generation) { store.refreshCloudMachine() }
    var confirmSnapshots by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    val offer = state.cloudOffer ?: return
    val machine = offer.cloudComputer
    if (machine == null && !offer.available) return
    val busy = "cloudMachine" in state.busy || "session" in state.busy
    CardSection("Codync Cloud", "A computer Codync runs for you in the cloud. It sleeps when idle and wakes when you message it.") {
        if (machine == null) row { CloudAction("cloud", "Create cloud computer", Palette.text, busy, spinning = busy, action = store::createCloudComputer) }
        else {
            row {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Symbol("cloud", size = 20.dp, tint = Palette.text)
                    Text(tr("Runs on {provider} · {region}").replace("{provider}", machine.providerName).replace("{region}", machine.region),
                        style = Type.body, color = Palette.text)
                }
            }
            if (state.preparingCloud) row {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Spinner(16.dp)
                    Text(tr("Preparing your cloud computer…"), style = Type.body, color = Palette.secondary)
                }
            } else row {
                ValueRow("State") {
                    Text(tr(machine.stateLabel), style = Type.body, color = if (machine.failed) Palette.danger else Palette.secondary)
                }
            }
            row { CloudAction("clock.arrow.circlepath", "Delete snapshots", Palette.danger, busy) { confirmSnapshots = true } }
            row { CloudAction("trash", "Delete cloud computer", Palette.danger, busy) { confirmDelete = true } }
        }
    }
    CodyncDialog("Delete snapshots?", confirmSnapshots, { confirmSnapshots = false },
        "Erases your cloud computer's earlier saved states and its backup. The computer and what's on it now stay.") {
        listOf(DialogAction("Delete", destructive = true, action = store::deleteCloudSnapshots))
    }
    CodyncDialog("Delete your cloud computer?", confirmDelete, { confirmDelete = false },
        "Erases everything on it: bots, chats, files and sign-ins. This can't be undone.") {
        listOf(DialogAction("Delete", destructive = true, action = store::deleteCloudComputer))
    }
}

@Composable private fun CloudAction(icon: String, title: String, ink: Color, busy: Boolean, spinning: Boolean = false, action: () -> Unit) {
    Row(Modifier.fillMaxWidth().plainButton(enabled = !busy, action = action), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Symbol(icon, size = 20.dp, tint = ink)
        Text(tr(title), Modifier.weight(1f), style = Type.body, color = ink)
        if (spinning) Spinner()
    }
}
