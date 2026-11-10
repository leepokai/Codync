package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import com.codync.android.core.CloudMachine
import com.codync.android.core.CloudPlacement
import com.codync.android.design.*

/** "Runs on Boat · EU": under the computer's name. */
@Composable internal fun CloudPlacement.runsOn(): String = tr("Runs on {provider} · {region}").replace("{provider}", providerName).replace("{region}", region)

private val CloudMachine.stateLabel: String get() = when {
    preparing -> "Preparing your cloud computer…"
    state == "starting" -> "Starting"
    state == "running" -> "Running"
    state == "stopping" -> "Going to sleep"
    state == "stopped" -> "Sleeping"
    state == "failed" -> "Failed"
    else -> state
}

/**
 * Computers → Codync Cloud: the computers Codync runs for this account on the user's own provider accounts, each with its
 * menu (rename, its provider's key, delete its snapshots, delete it), and "+" for another on the provider picked in New
 * cloud computer, whose keys open from there too. Shown only to an account that has one or may create one (the feature
 * isn't public yet).
 */
@Composable internal fun CloudSettings(state: AccountContext, store: AccountStore) {
    LaunchedEffect(state.generation) { store.refreshCloudMachines() }
    DisposableEffect(store) {
        store.cloudCardShown(true)
        onDispose { store.cloudCardShown(false) }
    }
    var confirmSnapshots by remember { mutableStateOf<CloudMachine?>(null) }
    var confirmDelete by remember { mutableStateOf<CloudMachine?>(null) }
    var naming by remember { mutableStateOf<Naming?>(null) }
    // The provider whose key sheet is open.
    var keyFor by remember { mutableStateOf<String?>(null) }
    var showsNew by remember { mutableStateOf(false) }
    val offer = state.cloudOffer ?: return
    if (offer.machines.isEmpty() && !offer.available) return
    val keyTemplate = tr("{provider} API key")
    // Codync Cloud is a beta feature.
    CardSection("Codync Cloud", "Runs on your own account with a cloud provider, which bills you while it's awake. It sleeps when idle and wakes when you message it.",
        accessory = {
            BetaTag()
            // At the limit too: the sheet says why it can't make another, and keeps every provider's key in reach.
            if (offer.available) IconButton("New cloud computer", "plus", enabled = CLOUD_CREATE !in state.busy) { showsNew = true }
        }) {
        for (machine in offer.machines) {
            val problem = offer.providers.firstOrNull { it.provider == machine.provider }?.credential?.problem
            row {
                MachineRow(machine, problem, state.cloudMachineBusy(machine.cloudComputerId)) { buildList {
                    add(MenuItem("Rename…", icon = "pencil") {
                        naming = Naming("Rename cloud computer", machine.name) { store.renameCloudComputer(machine.cloudComputerId, it) }
                    })
                    // Its provider's key, to replace it or see what to fix.
                    add(MenuItem(keyTemplate.replace("{provider}", machine.providerName), icon = "key.fill", verbatim = true) { keyFor = machine.provider })
                    // None yet while it prepares.
                    if (machine.snapshots) add(MenuItem("Delete snapshots", icon = "clock.arrow.circlepath", enabled = !machine.preparing) { confirmSnapshots = machine })
                    add(MenuItem("Delete cloud computer", icon = "trash", destructive = true) { confirmDelete = machine })
                } }
            }
        }
        state.cloudErrorAt(CloudErrorPlace.Card)?.let { row { Text(tr(it), style = Type.footnote, color = Palette.danger) } }
    }
    // A provider's key opens over New cloud computer from its row, and the list there follows the saved key.
    CodyncSheet(showsNew, { showsNew = false }) { NewCloudComputerSheet(offer, state, store) { keyFor = it } }
    CodyncSheet(keyFor, { keyFor = null }) { CloudKeySheet(it, offer, state, store) }
    NameDialog(naming, "Name", { naming = null }, { it.take(CLOUD_NAME_LIMIT) })
    val snapshots = lastOf(confirmSnapshots)
    CodyncDialog("Delete snapshots?", confirmSnapshots != null, { confirmSnapshots = null },
        "Erases your cloud computer's earlier saved states and its backup. The computer and what's on it now stay.") {
        listOf(DialogAction("Delete", destructive = true) { snapshots?.let { store.deleteCloudSnapshots(it.cloudComputerId) } })
    }
    val deleting = lastOf(confirmDelete)
    CodyncDialog(tr("Delete {name}?").replace("{name}", deleting?.name.orEmpty()), confirmDelete != null, { confirmDelete = null },
        "Erases everything on it: bots, chats, files and sign-ins. This can't be undone.") {
        listOf(DialogAction("Delete", destructive = true) { deleting?.let { store.deleteCloudComputer(it.cloudComputerId) } })
    }
}

/** [item] while it's set, else the last one: a dialog keeps its words while it fades away. */
@Composable private fun <T : Any> lastOf(item: T?): T? {
    var last by remember { mutableStateOf(item) }
    if (item != null) last = item
    return last
}

/** A cloud computer: its provider's logo, its name, where it runs and how it is (or what to fix while its provider turns
 * the key down: [problem], nothing wakes or sleeps it until then); its menu, or a spinner while one of its actions runs. */
@Composable private fun MachineRow(machine: CloudMachine, problem: String?, busy: Boolean, menu: () -> List<MenuItem>) {
    val stateInk = if (problem != null || machine.failed) Palette.danger else Palette.secondary
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        ProviderIcon(machine.provider, 20.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(machine.name, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(buildAnnotatedString {
                append("${machine.providerName} · ${machine.region} · ")
                withStyle(SpanStyle(color = stateInk)) { append(tr(problem ?: machine.stateLabel)) }
            }, style = Type.footnote, color = Palette.secondary)
        }
        Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) {
            if (busy) Spinner()
            else DropdownMenu(menu, Modifier.size(36.dp), label = tr("More")) { Symbol("ellipsis", size = 18.dp, tint = Palette.secondary) }
        }
    }
}
