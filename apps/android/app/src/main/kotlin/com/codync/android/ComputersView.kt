package com.codync.android

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.CloudComputer
import com.codync.android.core.Computer
import com.codync.android.core.ConnectionRoute
import com.codync.android.core.LinkState
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Computer management: every computer this phone can use or ask for, and the way to pair another (iOS `ComputersView`). */
@Composable internal fun ComputersView(state: AppState, store: AppStore, pair: () -> Unit, ask: (CloudComputer) -> Unit, extensions: () -> Unit) {
    val signedIn = state.account.userId != null
    val paired = state.computers.firstOrNull()
    var confirmForget by remember { mutableStateOf(false) }
    var confirmRevoke by remember { mutableStateOf<CloudComputer?>(null) }
    var coloring by remember { mutableStateOf(false) }
    var voice by remember { mutableStateOf(false) }
    var network by remember { mutableStateOf(false) }
    LaunchedEffect(state.contextId) { store.refreshCloud() }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        PageHeader("Computer management", icon = "gearshape")
        // One card per computer, so each one's rows read as its own.
        PullToRefresh(store::refreshComputers, Modifier.weight(1f)) { CardForm {
            if (paired != null) {
                val inAccount = state.cloudComputers.firstOrNull { it.computerId == paired.id }
                CardSection {
                    computerRows(state, store, inAccount != null, ComputerActions(extensions, store::openScreen, voice = { voice = true },
                        network = { network = true }, color = { coloring = true }, ask = inAccount?.let { c -> { ask(c) } },
                        revoke = inAccount?.takeIf { it.isGranted }?.let { c -> { confirmRevoke = c } }) { confirmForget = true })
                }
            }
            for (computer in state.cloudComputers.filter { it.computerId != paired?.id }) CardSection {
                row {
                    if (state.isOlderCopy(computer)) OlderCopyRow(computer)
                    else AccountComputerRow(computer, state.access?.takeIf { it.computerId == computer.computerId }?.code,
                        askingAnother = state.askingAccess.let { it != null && it != computer.computerId },
                        ask = { ask(computer) }, revoke = { confirmRevoke = computer })
                }
            }
            val hidden = state.bots.filter { it.hidden && !it.deleted }
            if (hidden.isNotEmpty()) CardSection("Hidden bots") {
                for (bot in hidden) row { HiddenBotRow(state, store, bot) }
            }
            CardSection(footer = if (signedIn) "Computers in your account need your OK on the computer before this phone can use them."
                else "Each bot runs on its own computer. Sign in to see the computers in your account.") {
                row {
                    Row(Modifier.fillMaxWidth().plainButton(action = pair), verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Symbol("qrcode.viewfinder", size = 20.dp, tint = Palette.text)
                        Text(tr("Pair a computer"), style = Type.body, color = Palette.text)
                    }
                }
            }
        } }
    }
    CodyncSheet(voice, { voice = false }) { VoiceChatSettings(state, store) }
    CodyncSheet(network, { network = false }) { state.network?.let { NetworkSheet(it, store::refreshNetwork) } }
    SwatchPanel(coloring, paired?.color, { coloring = false }, store::setColor)
    CodyncDialog(tr("Remove {name}?").replace("{name}", paired?.name ?: "computer"), confirmForget, { confirmForget = false },
        "Its bots and conversations stay on that computer. You can pair again any time.") {
        listOf(DialogAction("Remove", destructive = true, action = store::removeComputer))
    }
    CodyncDialog("Revoke this phone's access to ${confirmRevoke?.name ?: "the computer"}?", confirmRevoke != null, { confirmRevoke = null },
        "You can ask for access again; the computer will show a new code to confirm.") {
        listOfNotNull(confirmRevoke?.let { target -> DialogAction("Revoke", destructive = true) { store.revokeAccess(target.computerId) } })
    }
}

/** What the paired computer's row and its menu open; `revoke` is set when this phone's account access can be revoked. */
private class ComputerActions(val extensions: () -> Unit, val screen: () -> Unit, val voice: () -> Unit, val network: () -> Unit, val color: () -> Unit,
    val ask: (() -> Unit)?, val revoke: (() -> Unit)?, val remove: () -> Unit)

/**
 * The paired computer as the rows of its card: its connection and whether it's in the account, then
 * Extensions, Voice chat (its keys live on the computer), Screen, whether it sends push alerts here
 * (saved with the computer, so another account's computer keeps its own), how it's reached, the Codync it runs
 * (and its update) and, for a stale pairing, Remove (iOS `ComputerRows`).
 */
private fun CardRows.computerRows(state: AppState, store: AppStore, inAccount: Boolean, actions: ComputerActions) {
    row { ComputerSummary(state, inAccount, actions) }
    if (state.connection is LinkState.Ready) {
        row { RowLabel("Extensions", "square.grid.2x2", chevron = true, modifier = Modifier.plainButton(action = actions.extensions)) }
        row { RowLabel("Voice chat", "waveform", chevron = true, modifier = Modifier.plainButton(action = actions.voice)) }
        if (state.screen != null) row { RowLabel("Screen", "display", chevron = true, modifier = Modifier.plainButton(action = actions.screen)) }
        if (state.network?.shown == true) row { RowLabel("Network", NETWORK_ICON, chevron = true, modifier = Modifier.plainButton(action = actions.network)) }
    }
    if (BuildConfig.PUSH_CONFIGURED) row {
        CodyncSwitch(state.computers.first().notifications, store::notifications, Modifier.fillMaxWidth()) {
            RowLabel("Push notifications", "bell", modifier = Modifier.weight(1f))
        }
    }
    row { ConnectionRow(state.computers.first(), store) }
    // Kept while an update asked from here restarts the computer: its spinner says it's updating.
    if ((state.connection is LinkState.Ready || state.isUpdating) && state.hello != null) row { CodyncVersionRow(state, store::requestHostUpdate) }
    if (state.isStale && !inAccount) row { RowLabel("Remove", "trash", Palette.danger, modifier = Modifier.plainButton(action = actions.remove)) }
}

/** Name, connection, and an access request when this phone can't reach the computer; a long press opens its menu. */
@Composable private fun ComputerSummary(state: AppState, inAccount: Boolean, actions: ComputerActions) {
    val computer = state.computers.first()
    val link = state.connection
    val online = link is LinkState.Ready
    val detail = if (link is LinkState.Ready) {
        val count = state.bots.count { !it.deleted && it.kind != "group" }
        "$count bot${if (count == 1) "" else "s"} · ${routeLabel(link.route, computer.cloud)}"
    } else computerStatusText(state)
    Column(Modifier.fillMaxWidth().contextActions(items = {
        listOfNotNull(
            MenuItem("Color", icon = "paintpalette", action = actions.color),
            if (online) MenuItem("Extensions", icon = "square.grid.2x2", action = actions.extensions) else null,
            if (online) MenuItem("Voice chat", icon = "waveform", action = actions.voice) else null,
            if (online && state.screen != null) MenuItem("Screen", icon = "display", action = actions.screen) else null,
            if (online && state.network?.shown == true) MenuItem("Network", icon = NETWORK_ICON, action = actions.network) else null,
            actions.revoke?.let { MenuItem("Revoke this phone's access", icon = "lock.slash", destructive = true, divider = true, action = it) },
            if (!inAccount) MenuItem("Remove", icon = "trash", destructive = true, divider = actions.revoke == null, action = actions.remove) else null,
        )
    }), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            ComputerBadge(state.hello?.text("device"), 32.dp, computer.color)
            Text(state.hello?.text("name") ?: computer.name, Modifier.weight(1f), style = Type.body.semibold, color = Palette.text)
            if (inAccount) Symbol("person.crop.circle.badge.checkmark", size = 20.dp, tint = Palette.tertiary, contentDescription = "In your account")
        }
        val unreachable = (link is LinkState.ComputerOffline || link is LinkState.Failed) && !state.isUpdating
        Text(detail, style = Type.footnote, color = if (unreachable) Palette.warning else Palette.secondary)
        if (unreachable && actions.ask != null) {
            SecondaryButton(if (state.access?.computerId == computer.id) "Show code" else "Ask for access", action = actions.ask)
        }
    }
}

/** The Codync the computer runs, and its update when one is due (iOS `CodyncVersionRow`). */
@Composable private fun CodyncVersionRow(state: AppState, update: () -> Unit) {
    val line = state.hostVersionLine(rememberHostUpdateNow(state.hostUpdate)) ?: return
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.width(26.dp), contentAlignment = Alignment.Center) { Symbol("arrow.down.circle", size = 20.dp, tint = Palette.text) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Codync", style = Type.body, color = Palette.text)
                Text(line.version, style = Type.body, color = Palette.secondary)
            }
            if (line.detail.isNotEmpty()) Text(line.detail.map { tr(it) }.joinToString(" "), style = Type.footnote,
                color = if (line.warns) Palette.warning else Palette.secondary)
        }
        when (val accessory = line.accessory) {
            HostVersionLine.Accessory.Updating -> Spinner(16.dp)
            is HostVersionLine.Accessory.Button -> PrimaryButton(accessory.title, action = update)
            null -> {}
        }
    }
}

private const val NETWORK_ICON = "point.3.connected.trianglepath.dotted"

/** How this phone reaches the computer: the short name on the row, the whole route in the menu. */
@Composable private fun ConnectionRow(computer: Computer, store: AppStore) {
    DropdownMenu({ ConnectionRoute.entries.map { route -> MenuItem(routeTitle(route, computer.cloud), icon = route.icon, selected = route == computer.route) { store.setRoute(route) } } },
        Modifier.fillMaxWidth(), label = "Connection") {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(Modifier.width(26.dp), contentAlignment = Alignment.Center) { Symbol(computer.route.icon, size = 20.dp, tint = Palette.text) }
            Text(tr("Connection"), Modifier.weight(1f), style = Type.body, color = Palette.text)
            Text(tr(routeShortTitle(computer.route, computer.cloud)), style = Type.body, color = Palette.secondary, maxLines = 1)
            Symbol("chevron.down", size = 13.dp, tint = Palette.tertiary)
        }
    }
}

/** A computer in the account that this phone can't use yet; `askingAnother` while this phone asks a different one. */
@Composable private fun AccountComputerRow(computer: CloudComputer, code: String?, askingAnother: Boolean, ask: () -> Unit,
    revoke: () -> Unit) {
    val online = if (computer.online == true) "Online" else "Offline"
    val detail = when {
        code != null -> "Waiting for your OK on the computer · $code"
        computer.isGranted -> "Approved for an earlier install of Codync. Revoke it, then ask again."
        computer.access == "pending" -> "$online · A request is waiting"
        else -> "$online · In your account"
    }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.graphicsLayer { alpha = 0.5f }) { ComputerBadge(computer.device, 40.dp) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(computer.name, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(detail, style = Type.subheadline, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
        when {
            code != null -> SecondaryButton("Code", action = ask)
            computer.isGranted -> SecondaryButton("Revoke", action = revoke)
            else -> SecondaryButton("Ask for access", enabled = !askingAnother, action = ask)
        }
    }
}

/** An older account identity stays visible; only account management can remove it. */
@Composable private fun OlderCopyRow(computer: CloudComputer) {
    val seen = computer.lastSeenAt?.let { " · last seen ${RelativeTime.day(it)}" }.orEmpty()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.graphicsLayer { alpha = 0.35f }) { ComputerBadge(computer.device, 40.dp) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(computer.name, style = Type.body.semibold, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("Older copy of this computer$seen", style = Type.subheadline, color = Palette.tertiary, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable private fun HiddenBotRow(state: AppState, store: AppStore, bot: Bot) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        BotAvatar(bot, state.bots, 28.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(bot.name, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(state.hello?.text("name") ?: state.computers.firstOrNull()?.name.orEmpty(), style = Type.caption, color = Palette.tertiary)
        }
        IconButton("Unhide", "eye") {
            store.command("updateBot", buildJsonObject { put("id", bot.id); put("hidden", false) }, "roster:" + bot.id)
        }
    }
}

/** A computer's icon showing what it is (laptop, Mac mini, Linux…) in the color picked for it, as CodyncUI `ComputerBadge`. */
@Composable internal fun ComputerBadge(device: String?, size: Dp, color: String? = null) {
    val tint = color?.let(AvatarPalette::tint) ?: Palette.text
    Box(Modifier.size(size), contentAlignment = Alignment.Center) {
        if (device == "linux") Symbol("computer.linux", size = size * 0.55f, tint = tint)
        else Symbol(when (device) {
            "laptop" -> "laptopcomputer"
            "macmini" -> "macmini"
            "macstudio" -> "macstudio"
            "macpro" -> "macpro.gen3"
            else -> "desktopcomputer"
        }, size = size * 0.6f, tint = tint)
    }
}

/** The computer color choices over the dimmed app: a grid of colored dots, the current one checked. */
@Composable private fun SwatchPanel(isPresented: Boolean, selected: String?, close: () -> Unit, pick: (String) -> Unit) {
    val visible = remember { MutableTransitionState(false) }
    visible.targetState = isPresented
    if (!visible.currentState && !visible.targetState) return
    BackHandler(isPresented, close)
    Overlay {
        AnimatedVisibility(visible, enter = fadeIn(Motion.layout()), exit = fadeOut(Motion.fade())) {
            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.35f)).clickable(remember { MutableInteractionSource() }, null, onClick = close),
                contentAlignment = Alignment.Center) {
                Column(Modifier.graphicsLayer { shadowElevation = 24f; shape = RoundedRectangle(22.dp); clip = true }.background(Palette.bubbleAgent)
                    .clickable(remember { MutableInteractionSource() }, null) {}.semantics { paneTitle = "Color" }.padding(14.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    for (swatches in AvatarPalette.colors.entries.chunked(6)) Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        for ((id, hex) in swatches) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) {
                            Box(Modifier.size(32.dp).clip(Capsule()).background(Color(hex)).clickable(role = Role.RadioButton) { close(); pick(id) }
                                .semantics { contentDescription = id.replaceFirstChar(Char::uppercase); this.selected = id == selected },
                                contentAlignment = Alignment.Center) {
                                if (id == selected) Symbol("checkmark", size = 13.dp, tint = Color.White)
                            }
                        }
                    }
                }
            }
        }
    }
}

private val ConnectionRoute.icon get() = when (this) {
    ConnectionRoute.Automatic -> "arrow.triangle.branch"
    ConnectionRoute.CloudflareFirst -> "cloud"
    ConnectionRoute.DirectOnly -> "wifi"
}
