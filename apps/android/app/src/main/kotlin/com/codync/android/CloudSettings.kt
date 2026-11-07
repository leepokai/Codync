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
import com.codync.android.core.CloudOffer
import com.codync.android.design.*

/** "Runs on Boat · EU": under the cloud computer's name, which is Codync Cloud already. */
@Composable internal fun CloudMachine.runsOn(): String = tr("Runs on {provider} · {region}").replace("{provider}", providerName).replace("{region}", region)

private val CloudMachine.stateLabel: String get() = when (state) {
    "starting" -> "Starting"
    "running" -> "Running"
    "stopping" -> "Going to sleep"
    "stopped" -> "Sleeping"
    "failed" -> "Failed"
    else -> state
}

/**
 * Computers → Codync Cloud: the computer Codync runs for this account on the user's own provider account, creating it
 * on a connected provider, and the Cloud providers sheet with their keys. Shown only to an account that has one or may
 * create one (the feature isn't public yet).
 */
@Composable internal fun CloudSettings(state: AccountContext, store: AccountStore) {
    LaunchedEffect(state.generation) { store.refreshCloudMachine() }
    var confirmSnapshots by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    var showsProviders by remember { mutableStateOf(false) }
    val offer = state.cloudOffer ?: return
    val machine = offer.cloudComputer
    if (machine == null && !offer.available) return
    val busy = "cloudMachine" in state.busy || "session" in state.busy
    // Codync Cloud is a beta feature.
    CardSection("Codync Cloud", "Runs on your own account with a cloud provider, which bills you while it's awake. It sleeps when idle and wakes when you message it.",
        accessory = { BetaTag() }) {
        if (machine == null) {
            row { ProvidersRow(offer) { showsProviders = true } }
            if (offer.connected.isNotEmpty()) row { CreateRow(offer, busy, store) }
        } else {
            row {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Symbol("cloud", size = 20.dp, tint = Palette.text)
                    Text(machine.runsOn(),
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
            row { ProvidersRow(offer) { showsProviders = true } }
        }
    }
    CodyncSheet(showsProviders, { showsProviders = false }) { CloudProvidersSheet(offer, state.busy, state.error, store) }
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
    CloudActionLabel(icon, title, ink, spinning, Modifier.plainButton(enabled = !busy, action = action))
}

/** A [CloudAction]'s row without its tap, for a menu that takes it. */
@Composable private fun CloudActionLabel(icon: String, title: String, ink: Color, spinning: Boolean, modifier: Modifier = Modifier) {
    Row(Modifier.fillMaxWidth().then(modifier), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Symbol(icon, size = 20.dp, tint = ink)
        Text(tr(title), Modifier.weight(1f), style = Type.body, color = ink)
        if (spinning) Spinner()
    }
}

/** Opens the Cloud providers sheet; says which providers have a key, or that one needs attention. */
@Composable private fun ProvidersRow(offer: CloudOffer, open: () -> Unit) {
    val (summary, needsAttention) = offer.summary
    Row(Modifier.fillMaxWidth().plainButton(action = open), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Symbol("key.fill", size = 20.dp, tint = Palette.text)
        Text(tr("Cloud providers"), Modifier.weight(1f), style = Type.body, color = Palette.text)
        Text(tr(summary), style = Type.body, color = if (needsAttention) Palette.danger else Palette.secondary, maxLines = 1)
        Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
    }
}

/** Creates the computer on the one connected provider, or on the one picked from the menu. */
@Composable private fun CreateRow(offer: CloudOffer, busy: Boolean, store: AccountStore) {
    val connected = offer.connected
    val only = connected.singleOrNull()
    if (only != null) return CloudAction("cloud", "Create cloud computer", Palette.text, busy, spinning = busy) { store.createCloudComputer(only.provider) }
    val onProvider = tr("On {provider}")
    DropdownMenu({ connected.map { p -> MenuItem(onProvider.replace("{provider}", p.name), verbatim = true) { store.createCloudComputer(p.provider) } } },
        Modifier.fillMaxWidth(), enabled = !busy, label = "Create cloud computer") {
        CloudActionLabel("cloud", "Create cloud computer", Palette.text, spinning = busy)
    }
}
