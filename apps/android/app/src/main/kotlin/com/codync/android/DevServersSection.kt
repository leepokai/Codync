package com.codync.android

import android.content.ClipData
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.DevServer
import com.codync.android.core.DevServerAction
import com.codync.android.core.DevServerMenuRow
import com.codync.android.core.DevServerStatus
import com.codync.android.core.DevServerTap
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/** What a bot keeps running (iOS `DevServersSection`): a row is its name and ports, tapping it opens it, the rest is behind `⋯`. */
@Composable internal fun DevServersSection(bot: Bot, store: AppStore) {
    var logs by remember { mutableStateOf<DevServer?>(null) }
    CardSection("Dev servers") {
        for (server in bot.devServers) row { DevServerRow(bot.id, server, store) { logs = server } }
    }
    CodyncSheet(logs, { logs = null }) { DevServerLogsView(bot.id, it, store) }
}

@Composable private fun DevServerRow(botId: String, server: DevServer, store: AppStore, showLogs: () -> Unit) {
    val uri = LocalUriHandler.current
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    fun open(address: String) {
        try {
            uri.openUri(address)
            Analytics.capture(AnalyticsEvent.DevServerAction(DevServerUse.Open))
        } catch (failure: Exception) { store.showError(failure) }
    }
    fun copy(address: String) {
        Analytics.capture(AnalyticsEvent.DevServerAction(DevServerUse.Copy))
        scope.launch { clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("Address", address))) }
    }
    val tap: (() -> Unit)? = when (val action = server.tap) {
        is DevServerTap.Open -> { { open(action.address) } }
        DevServerTap.Logs -> showLogs
        null -> null
    }
    Row(Modifier.fillMaxWidth().heightIn(min = 32.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.weight(1f).then(if (tap != null) Modifier.plainButton(action = tap) else Modifier),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(server.name, Modifier.weight(1f), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (server.status == DevServerStatus.Starting) Spinner(11.dp)
            Text(tr(server.rowValue), style = Type.body.monospacedDigit, maxLines = 1,
                color = if (server.status == DevServerStatus.Failed) Palette.danger else Palette.secondary)
        }
        // A 44 dp target in a row of the card's height, its glyph near the card's edge.
        DropdownMenu({ menuItems(botId, server, store, showLogs, ::open, ::copy) }, Modifier.offset(x = 12.dp).height(32.dp).requiredSize(44.dp),
            label = tr("More actions for {name}").replace("{name}", server.name)) {
            Symbol("ellipsis", size = 18.dp, tint = Palette.secondary)
        }
    }
}

private fun menuItems(botId: String, server: DevServer, store: AppStore, showLogs: () -> Unit, open: (String) -> Unit, copy: (String) -> Unit): List<MenuItem> {
    fun act(action: DevServerAction) = store.devServerAction(action, botId, server.id)
    return server.menuRows.mapIndexed { index, row ->
        when (row) {
            is DevServerMenuRow.OpenAddress -> MenuItem(row.title, icon = "arrow.up.right", verbatim = true) { open(row.address) }
            DevServerMenuRow.OnlyOnComputer -> MenuItem("Only reachable on the computer", enabled = false)
            DevServerMenuRow.StartedByShell -> MenuItem("Started through the bot's shell: it stops with the bot", enabled = false)
            DevServerMenuRow.Open -> MenuItem("Open", icon = "arrow.up.right") { server.remoteAddress?.let(open) }
            DevServerMenuRow.CopyAddress -> MenuItem("Copy address", icon = "doc.on.doc") { server.remoteAddress?.let(copy) }
            DevServerMenuRow.Logs -> MenuItem("Logs", icon = "doc.text", action = showLogs)
            DevServerMenuRow.Restart -> MenuItem("Restart", icon = "arrow.clockwise") { act(DevServerAction.Restart) }
            DevServerMenuRow.Start -> MenuItem("Start", icon = "arrow.clockwise") { act(DevServerAction.Restart) }
            DevServerMenuRow.Stop -> MenuItem("Stop", icon = "stop.fill", destructive = true, divider = index > 0) { act(DevServerAction.Stop) }
            DevServerMenuRow.Remove -> MenuItem("Remove", icon = "trash", destructive = true, divider = index > 0) { act(DevServerAction.Remove) }
        }
    }
}

private sealed interface DevServerOutput {
    data object Loading : DevServerOutput
    data class Shown(val text: String) : DevServerOutput
    data class Failed(val message: String) : DevServerOutput
}

/** A server's recent output, to read. */
@Composable private fun DevServerLogsView(botId: String, server: DevServer, store: AppStore) {
    // Moves to read the logs again.
    var reads by remember(server.id) { mutableIntStateOf(0) }
    var output by remember(server.id) { mutableStateOf<DevServerOutput>(DevServerOutput.Loading) }
    LaunchedEffect(server.id, reads) {
        output = try { DevServerOutput.Shown(store.devServerLogs(botId, server.id)) }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { DevServerOutput.Failed(failure.message ?: failure.toString()) }
    }
    val exit = server.exitCode?.takeIf { server.status == DevServerStatus.Failed }?.let { " · " + tr("Exit code {code}").replace("{code}", "$it") }.orEmpty()
    Column(Modifier.fillMaxSize()) {
        ModalHeader({
            Column {
                Text(server.name, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(server.command + exit, style = Type.system(11f).monospaced, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            }
        }, { IconButton("Refresh", "arrow.clockwise") { reads++ } })
        when (val shown = output) {
            is DevServerOutput.Shown -> if (shown.text.isEmpty()) LogsState { Text(tr("No output yet"), style = Type.body, color = Palette.secondary) }
                else SelectionContainer(Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState())) {
                    Text(shown.text, Modifier.padding(start = 20.dp, end = 20.dp, bottom = 20.dp), style = Type.system(12f).monospaced, color = Palette.text)
                }
            is DevServerOutput.Failed -> LogsState {
                Column(Modifier.padding(20.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    SelectionContainer { Text(tr(shown.message), style = Type.body, color = Palette.danger, textAlign = TextAlign.Center) }
                    SecondaryButton("Try again") { output = DevServerOutput.Loading; reads++ }
                }
            }
            DevServerOutput.Loading -> LogsState { Spinner() }
        }
    }
}

/** The sheet's body when there's no output to scroll. */
@Composable private fun LogsState(content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { content() }
}
