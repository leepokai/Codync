package com.codync.android

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
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
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.CloudComputer
import com.codync.android.core.ConnectionRoute
import com.codync.android.core.HostRoute
import com.codync.android.core.LinkState
import com.codync.android.design.*
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
    var confirmStartOver by remember { mutableStateOf(false) }
    var voice by remember { mutableStateOf(false) }
    var aboutAnalytics by remember { mutableStateOf(false) }
    LaunchedEffect(state.contextId) { store.refreshCloud() }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        PageHeader("Computers & settings", pop)
        CardForm(Modifier.weight(1f)) {
            CardSection("Computers", if (signedIn) "Computers in your account need your OK on the computer before this phone can use them."
                else "Each bot runs on its own computer. Sign in to see the computers in your account.") {
                if (paired != null) row {
                    ComputerRow(state, store, state.cloudComputers.any { it.computerId == paired.id }, extensions, store::openScreen) { confirmForget = true }
                }
                for (computer in state.cloudComputers.filter { it.computerId != paired?.id }) row {
                    AccountComputerRow(computer, state.access?.takeIf { it.computerId == computer.computerId }?.code, canAsk = paired == null) { ask(computer) }
                }
                row {
                    Row(Modifier.fillMaxWidth().plainButton(action = pair), verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Symbol("qrcode.viewfinder", size = 20.dp, tint = Palette.text)
                        Text("Pair a computer", style = Type.body, color = Palette.text)
                    }
                }
            }
            if (state.actualConnection is LinkState.Ready) CardSection("Voice chat", "Android speech on this phone, or OpenAI and Gemini on your own API key, kept on the computer.") {
                row {
                    Row(Modifier.fillMaxWidth().plainButton { voice = true }, verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Symbol("waveform", size = 20.dp, tint = Palette.text)
                        Text("Voice and API keys", Modifier.weight(1f), style = Type.body, color = Palette.text)
                        Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
                    }
                }
            }
            NotificationsSection(state, store)
            CardSection(footer = "Which features you use, never your messages, code or files. Helps us spot bugs sooner and make Codync better for you.") {
                row {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text("Share usage analytics", style = Type.body, color = Palette.text, maxLines = 1)
                        IconButton("About usage analytics", "info.circle") { aboutAnalytics = true }
                        Box(Modifier.weight(1f))
                        CodyncSwitch(Analytics.enabled, Analytics::setEnabled, Modifier.width(34.dp)
                            .semantics { contentDescription = "Share usage analytics" }) {}
                    }
                }
            }
            val hidden = state.bots.filter { it.hidden && !it.deleted }
            if (hidden.isNotEmpty()) CardSection("Hidden bots") {
                for (bot in hidden) row { HiddenBotRow(state, store, bot) }
            }
            CardSection {
                row { ValueRow("App version", BuildConfig.VERSION_NAME) }
                row { LinkButton("Source code") { uri.openUri("https://github.com/leepokai/Codync") } }
            }
            CardSection {
                row {
                    Row(Modifier.fillMaxWidth().plainButton(enabled = "session" !in state.busy) { confirmStartOver = true },
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Symbol("arrow.counterclockwise", size = 20.dp, tint = Palette.danger)
                        Text("Start over", style = Type.body, color = Palette.danger)
                    }
                }
            }
        }
    }
    CodyncSheet(voice, { voice = false }) { VoiceChatSettings(state, store) }
    CodyncSheet(aboutAnalytics, { aboutAnalytics = false }) { AnalyticsSheet() }
    CodyncDialog("Start over?", confirmStartOver, { confirmStartOver = false },
        "Signs out of every account and forgets every computer on this phone, then shows the welcome again. Your computers keep their bots and chats; pair again to use them.") {
        listOf(DialogAction("Start over", destructive = true, action = store::startOver))
    }
    CodyncDialog("Remove ${paired?.name ?: "computer"}?", confirmForget, { confirmForget = false },
        "Its bots and conversations stay on that computer. You can pair again any time.") {
        listOf(DialogAction("Remove", destructive = true, action = store::forgetComputer))
    }
}

/** Push on or off for this phone, and whether Android lets Codync show notifications at all. */
@Composable private fun NotificationsSection(state: AppState, store: AppStore) {
    val context = LocalContext.current
    var asked by remember { mutableStateOf(false) }
    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { asked = true }
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

/** The paired computer: its connection, how it's reached, and whether it's in the account. */
@Composable private fun ComputerRow(state: AppState, store: AppStore, inAccount: Boolean, extensions: () -> Unit, screen: () -> Unit,
    remove: () -> Unit) {
    val computer = state.computers.first()
    val link = state.connection
    val online = link is LinkState.Ready
    val viewable = online && state.screen != null
    val detail = if (link is LinkState.Ready) {
        val count = state.bots.count { !it.deleted && it.kind != "group" }
        "$count bot${if (count == 1) "" else "s"} · ${if (link.route == HostRoute.Direct) "Wi-Fi/Tailscale" else "Cloudflare"}"
    } else connectionLabel(link)
    Row(Modifier.fillMaxWidth().contextActions(items = {
        listOfNotNull(
            if (online) MenuItem("Extensions", icon = "square.grid.2x2", action = extensions) else null,
            if (viewable) MenuItem("Screen", icon = "display", action = screen) else null,
            MenuItem("Remove", icon = "trash", destructive = true, divider = online, action = remove),
        )
    }), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        ComputerBadge(state.hello?.text("device"), 40.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(state.hello?.text("name") ?: computer.name, style = Type.body.semibold, color = Palette.text, maxLines = 1)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                val ink = if (link is LinkState.ComputerOffline || link is LinkState.Failed) Palette.warning else Palette.secondary
                Text(detail, style = Type.subheadline, color = ink, maxLines = 1)
                if (link is LinkState.Ready) Symbol(if (link.route == HostRoute.Direct) "wifi" else "cloud", size = 14.dp, tint = ink,
                    contentDescription = if (link.route == HostRoute.Direct) "Wi-Fi or Tailscale" else "Through Cloudflare")
            }
        }
        if (online) IconButton("Extensions", "square.grid.2x2", action = extensions)
        if (viewable) IconButton("Screen", "display", action = screen)
        // Which route goes first, and whether Cloudflare is a fallback at all.
        DropdownMenu({ ConnectionRoute.entries.map { route -> MenuItem(route.title, icon = route.icon, selected = route == computer.route) { store.setRoute(route) } } },
            Modifier.size(36.dp), label = "Connection") {
            Symbol(computer.route.icon, size = 21.dp, tint = Palette.secondary)
        }
        if (inAccount) Symbol("person.crop.circle.badge.checkmark", size = 20.dp, tint = Palette.tertiary, contentDescription = "In your account")
    }
}

/** A computer in the account that this phone can't use yet. */
@Composable private fun AccountComputerRow(computer: CloudComputer, code: String?, canAsk: Boolean, ask: () -> Unit) {
    val online = if (computer.online == true) "Online" else "Offline"
    val detail = when {
        code != null -> "Waiting for your OK on the computer · $code"
        computer.access == "granted" -> "Approved for an earlier install of Codync. Ask again to use it here."
        computer.access == "pending" -> "$online · A request is waiting"
        canAsk -> "$online · In your account"
        // One computer per phone for now: the paired one goes first.
        else -> "$online · Remove this phone's computer to use this one"
    }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.graphicsLayer { alpha = 0.5f }) { ComputerBadge(computer.device, 40.dp) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(computer.name, style = Type.body.semibold, color = Palette.text, maxLines = 1)
            Text(detail, style = Type.subheadline, color = Palette.secondary, maxLines = 2)
        }
        SecondaryButton(if (code != null) "Code" else "Ask for access", enabled = code != null || canAsk, action = ask)
    }
}

@Composable private fun HiddenBotRow(state: AppState, store: AppStore, bot: Bot) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        BotAvatar(bot, state.bots, 28.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(bot.name, style = Type.body, color = Palette.text, maxLines = 1)
            Text(state.hello?.text("name") ?: state.computers.firstOrNull()?.name.orEmpty(), style = Type.caption, color = Palette.tertiary)
        }
        IconButton("Unhide", "eye") {
            store.command("updateBot", buildJsonObject { put("id", bot.id); put("hidden", false) }, "roster:" + bot.id)
        }
    }
}

/** A computer's icon showing what it is (laptop, desktop, Linux), as CodyncUI `ComputerBadge`. */
@Composable internal fun ComputerBadge(device: String?, size: Dp) {
    Box(Modifier.size(size), contentAlignment = Alignment.Center) {
        when (device) {
            "linux" -> Symbol("computer.linux", size = size * 0.55f, tint = Palette.text)
            "laptop" -> Symbol("laptopcomputer", size = size * 0.6f, tint = Palette.text)
            else -> Symbol("desktopcomputer", size = size * 0.6f, tint = Palette.text)
        }
    }
}

private val ConnectionRoute.title get() = when (this) {
    ConnectionRoute.Automatic -> "Auto: Wi-Fi or Tailscale, then Cloudflare"
    ConnectionRoute.CloudflareFirst -> "Cloudflare, then Wi-Fi or Tailscale"
    ConnectionRoute.DirectOnly -> "Wi-Fi or Tailscale only, never Cloudflare"
}

private val ConnectionRoute.icon get() = when (this) {
    ConnectionRoute.Automatic -> "arrow.triangle.branch"
    ConnectionRoute.CloudflareFirst -> "cloud"
    ConnectionRoute.DirectOnly -> "wifi"
}
