package com.codync.android

import android.Manifest
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.foundation.layout.Spacer
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
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
import com.codync.android.core.ConnectionRoute
import com.codync.android.core.HostRoute
import com.codync.android.core.LinkState
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/**
 * Every computer this phone can use or ask for, plus settings for the selected account
 * (iOS `SettingsView`). `pop` is set when it's pushed inside Accounts.
 */
@Composable internal fun SettingsView(state: AppState, store: AppStore, pop: (() -> Unit)?, pair: () -> Unit, ask: (CloudComputer) -> Unit,
    extensions: () -> Unit) {
    val uri = LocalUriHandler.current
    val signedIn = state.account.userId != null
    val paired = state.computers.firstOrNull()
    var confirmForget by remember { mutableStateOf(false) }
    var confirmRevoke by remember { mutableStateOf<CloudComputer?>(null) }
    var coloring by remember { mutableStateOf(false) }
    var confirmStartOver by remember { mutableStateOf(false) }
    var voice by remember { mutableStateOf(false) }
    var aboutAnalytics by remember { mutableStateOf(false) }
    LaunchedEffect(state.contextId) { store.refreshCloud() }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        PageHeader("Computers & settings", pop, icon = "gearshape")
        PullToRefresh(store::refreshComputers, Modifier.weight(1f)) { CardForm {
            CardSection("Computers", if (signedIn) "Computers in your account need your OK on the computer before this phone can use them."
                else "Each bot runs on its own computer. Sign in to see the computers in your account.") {
                if (paired != null) row {
                    val inAccount = state.cloudComputers.firstOrNull { it.computerId == paired.id }
                    ComputerRow(state, store, inAccount != null, ComputerActions(extensions, store::openScreen, voice = { voice = true }, color = { coloring = true },
                        ask = inAccount?.let { c -> { ask(c) } },
                        revoke = inAccount?.takeIf { it.isGranted }?.let { c -> { confirmRevoke = c } }) { confirmForget = true })
                }
                for (computer in state.cloudComputers.filter { it.computerId != paired?.id }) row {
                    if (state.isOlderCopy(computer)) OlderCopyRow(computer)
                    else AccountComputerRow(computer, state.access?.takeIf { it.computerId == computer.computerId }?.code,
                        askingAnother = state.askingAccess.let { it != null && it != computer.computerId },
                        ask = { ask(computer) }, revoke = { confirmRevoke = computer })
                }
                row {
                    Row(Modifier.fillMaxWidth().plainButton(action = pair), verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Symbol("qrcode.viewfinder", size = 20.dp, tint = Palette.text)
                        Text(tr("Pair a computer"), style = Type.body, color = Palette.text)
                    }
                }
            }
            AppearanceSettings()
            if (state.actualConnection is LinkState.Ready) CardSection("Voice chat", "Android speech on this phone, or OpenAI and Gemini on your own API key, kept on the computer.") {
                row {
                    Row(Modifier.fillMaxWidth().plainButton { voice = true }, verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Symbol("waveform", size = 20.dp, tint = Palette.text)
                        Text(tr("Voice and API keys"), Modifier.weight(1f), style = Type.body, color = Palette.text)
                        Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
                    }
                }
            }
            val appLanguage = LocalAppLanguage.current
            val setAppLanguage = LocalSetAppLanguage.current
            val systemChinese = LocalSystemChinese.current
            val phrases = LocalPhrases.current
            CardSection("Language", footer = "Choose the language for this device. Unsupported system languages use English.") {
                row {
                    ValueRow("App language") {
                        ChoicePicker(appLanguage, AppLanguage.entries.map {
                            it to AppLanguage.menuTitle(it, appLanguage, systemChinese, phrases)
                        }, setAppLanguage)
                    }
                }
            }
            NotificationsSection(state, store)
            CardSection(footer = "Which features you use, never your messages, code or files. Helps us spot bugs sooner and make Codync better for you.") {
                row {
                    val analyticsLabel = tr("Share usage analytics")
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(analyticsLabel, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        IconButton("About usage analytics", "info.circle") { aboutAnalytics = true }
                        Box(Modifier.weight(1f))
                        CodyncSwitch(Analytics.enabled, Analytics::setEnabled, Modifier.width(34.dp)
                            .semantics { contentDescription = analyticsLabel }) {}
                    }
                }
            }
            val hidden = state.bots.filter { it.hidden && !it.deleted }
            if (hidden.isNotEmpty()) CardSection("Hidden bots") {
                for (bot in hidden) row { HiddenBotRow(state, store, bot) }
            }
            CardSection {
                row { ValueRow("App version", BuildConfig.VERSION_NAME) }
                row { LinkButton("Source code") { uri.open("https://github.com/leepokai/Codync", OutboundLink.Github) } }
            }
            CardSection {
                row {
                    Row(Modifier.fillMaxWidth().plainButton(enabled = "session" !in state.busy) { confirmStartOver = true },
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Symbol("arrow.counterclockwise", size = 20.dp, tint = Palette.danger)
                        Text(tr("Start over"), style = Type.body, color = Palette.danger)
                    }
                }
            }
        } }
    }
    CodyncSheet(voice, { voice = false }) { VoiceChatSettings(state, store) }
    SwatchPanel(coloring, paired?.color, { coloring = false }, store::setColor)
    CodyncSheet(aboutAnalytics, { aboutAnalytics = false }) { AnalyticsSheet() }
    CodyncDialog("Start over?", confirmStartOver, { confirmStartOver = false },
        "Signs out of every account and forgets every computer on this phone, then shows the welcome again. Your computers keep their bots and chats; pair again to use them.") {
        listOf(DialogAction("Start over", destructive = true, action = store::startOver))
    }
    CodyncDialog(tr("Remove {name}?").replace("{name}", paired?.name ?: "computer"), confirmForget, { confirmForget = false },
        "Its bots and conversations stay on that computer. You can pair again any time.") {
        listOf(DialogAction("Remove", destructive = true, action = store::removeComputer))
    }
    CodyncDialog("Revoke this phone's access to ${confirmRevoke?.name ?: "the computer"}?", confirmRevoke != null, { confirmRevoke = null },
        "You can ask for access again; the computer will show a new code to confirm.") {
        listOfNotNull(confirmRevoke?.let { target -> DialogAction("Revoke", destructive = true) { store.revokeAccess(target.computerId) } })
    }
}

/** Push on or off for this phone, and whether Android lets Codync show notifications at all. */
@Composable private fun NotificationsSection(state: AppState, store: AppStore) {
    val context = LocalContext.current
    var asked by remember { mutableStateOf(false) }
    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        notificationsAnswered(context, granted, PromptSource.Settings); asked = true
    }
    val allowed = rememberOnResume { notificationsAllowed(context) } || asked && notificationsAllowed(context)
    CardSection("Notifications", "Get a result summary, a request for input, or a failure notice. Notification previews follow your Android settings.") {
        if (BuildConfig.PUSH_CONFIGURED) row { SwitchRow("Push notifications", isOn = state.notifications, onChange = store::notifications) }
        else row { Text("Push isn't set up in this build. Bots and chats still update while Codync is open.", style = Type.subheadline, color = Palette.secondary) }
        row {
            ValueRow("Notifications", if (allowed) "On" else if (asked) "Off in Settings" else "Turn on", Modifier.plainButton {
                if (!allowed && !asked && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) request.launch(Manifest.permission.POST_NOTIFICATIONS)
                else openNotificationSettings(context)
            })
        }
    }
}

/** What the paired computer's row and its menu open; `revoke` is set when this phone's account access can be revoked. */
private class ComputerActions(val extensions: () -> Unit, val screen: () -> Unit, val voice: () -> Unit, val color: () -> Unit,
    val ask: (() -> Unit)?, val revoke: (() -> Unit)?, val remove: () -> Unit)

/** The paired computer: its connection, how it's reached, and whether it's in the account. */
@Composable private fun ComputerRow(state: AppState, store: AppStore, inAccount: Boolean, actions: ComputerActions) {
    val computer = state.computers.first()
    val link = state.connection
    val online = link is LinkState.Ready
    val viewable = online && state.screen != null
    val detail = if (link is LinkState.Ready) {
        val count = state.bots.count { !it.deleted && it.kind != "group" }
        val via = if (link.route == HostRoute.Direct) "Wi-Fi/Tailscale" else relayHost(computer.cloud)?.let { "Private relay ($it)" } ?: "Cloudflare"
        "$count bot${if (count == 1) "" else "s"} · $via"
    } else computerStatusText(state)
    Column(Modifier.fillMaxWidth().contextActions(items = {
        listOfNotNull(
            MenuItem("Color", icon = "paintpalette", action = actions.color),
            if (online) MenuItem("Extensions", icon = "square.grid.2x2", action = actions.extensions) else null,
            if (online) MenuItem("Voice chat", icon = "waveform", action = actions.voice) else null,
            if (viewable) MenuItem("Screen", icon = "display", action = actions.screen) else null,
            actions.revoke?.let { MenuItem("Revoke this phone's access", icon = "lock.slash", destructive = true, divider = true, action = it) },
            if (!inAccount) MenuItem("Remove", icon = "trash", destructive = true, divider = actions.revoke == null, action = actions.remove) else null,
        )
    }), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            ComputerBadge(state.hello?.text("device"), 32.dp, computer.color)
            Text(state.hello?.text("name") ?: computer.name, Modifier.weight(1f), style = Type.body.semibold, color = Palette.text)
            if (inAccount) Symbol("person.crop.circle.badge.checkmark", size = 20.dp, tint = Palette.tertiary, contentDescription = "In your account")
        }
        if (online) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(detail, Modifier.weight(1f), style = Type.footnote, color = Palette.secondary)
                ComputerControls(state, store, inAccount, actions)
            }
        } else {
            val ink = if (link is LinkState.ComputerOffline || link is LinkState.Failed) Palette.warning else Palette.secondary
            Text(detail, style = Type.footnote, color = ink)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if ((link is LinkState.Failed || link is LinkState.ComputerOffline) && actions.ask != null) {
                    SecondaryButton(if (state.access?.computerId == computer.id) "Show code" else "Ask for access", action = actions.ask)
                }
                Spacer(Modifier.weight(1f))
                ComputerControls(state, store, inAccount, actions)
            }
        }
    }
}

/** Keep the computer's actions together beside its connection summary. */
@Composable private fun ComputerControls(state: AppState, store: AppStore, inAccount: Boolean, actions: ComputerActions) {
    val computer = state.computers.first()
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        if (state.connection is LinkState.Ready) {
            IconButton("Extensions", "square.grid.2x2", action = actions.extensions)
            if (state.screen != null) IconButton("Screen", "display", action = actions.screen)
        }
        DropdownMenu({ ConnectionRoute.entries.map { route -> MenuItem(routeTitle(route, computer.cloud), icon = route.icon, selected = route == computer.route) { store.setRoute(route) } } },
            Modifier.size(36.dp), label = "Connection") {
            Symbol(computer.route.icon, size = 21.dp, tint = Palette.secondary)
        }
        if (state.isStale && !inAccount) IconButton("Remove", "trash", action = actions.remove)
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
