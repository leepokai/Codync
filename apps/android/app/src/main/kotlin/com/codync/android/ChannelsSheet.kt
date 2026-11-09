package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.Channels
import com.codync.android.core.DiscordStatus
import com.codync.android.core.GitHubStatus
import com.codync.android.core.SlackStatus
import com.codync.android.core.TelegramBot
import com.codync.android.design.*
import kotlinx.coroutines.launch

/**
 * The messaging channels of the computer (Telegram bots and Slack): each bot's state, when it last heard from someone,
 * who may write to it and which bot answers. A phone can pair someone and take people out; adding a Telegram bot or
 * removing one needs its token, which stays on the computer.
 */
@Composable internal fun ChannelsSheet(state: AppState, store: AppStore) {
    var bots by remember { mutableStateOf<List<TelegramBot>?>(null) }
    var slack by remember { mutableStateOf<SlackStatus?>(null) }
    var discord by remember { mutableStateOf<DiscordStatus?>(null) }
    var github by remember { mutableStateOf<GitHubStatus?>(null) }
    var pairing by remember { mutableStateOf<Pair<String, String>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val computer = state.computers.firstOrNull()?.id
    LaunchedEffect(computer) {
        bots = store.telegramBots()
        slack = store.slackStatus()
        discord = store.discordStatus()
        github = store.githubStatus()
    }
    // One change at a time; the host's answer replaces what is shown.
    fun act(work: suspend () -> Unit) {
        if (busy) return
        busy = true; error = null
        scope.launch {
            try { work() } catch (e: Exception) { error = e.message ?: "That did not work." } finally { busy = false }
        }
    }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Channels")
        CardForm {
            val list = bots
            CardSection(tr("Telegram"), footer = tr("Add a Telegram bot, or remove one, on the computer: its token stays there.")) {
                if (list == null) row { Text(tr("Loading…"), style = Type.body, color = Palette.secondary) }
                else if (list.isEmpty()) row { Text(tr("No Telegram bots yet."), style = Type.body, color = Palette.secondary) }
                else for (bot in list) {
                    row {
                        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            val handle = if (bot.username.isEmpty()) "" else " · @${bot.username}"
                            Text("${bot.name}$handle → ${bot.botName}", style = Type.body, color = Palette.text)
                            Text("${tr(Channels.stateText(bot.state))} · ${tr(Channels.lastReceivedText(bot.lastReceivedAt, System.currentTimeMillis()))} · ${tr(Channels.allowedText(bot.allowed.size))}",
                                style = Type.caption, color = Palette.secondary)
                            for (id in bot.allowed) {
                                ValueRow("Telegram user $id") {
                                    SecondaryButton(tr("Remove"), enabled = !busy) { act { bots = store.setTelegramAllowed(bot.name, id, false) } }
                                }
                            }
                            pairing?.takeIf { it.first == bot.name }?.let { (_, code) ->
                                Text(tr(Channels.pairingInstruction(code, bot.username)), style = Type.footnote, color = Palette.text)
                            }
                            SecondaryButton(tr("Pair someone"), enabled = !busy) {
                                act {
                                    val answer = store.pairTelegram(bot.name)
                                    Channels.pairingCode(answer)?.let { pairing = bot.name to it }
                                    bots = store.telegramBots() ?: bots
                                }
                            }
                        }
                    }
                }
            }
            CardSection(tr("Discord"), footer = tr("Set up Discord on the computer: its bot token stays there. Only the people allowed in each channel can write to it.")) {
                row {
                    val connected = discord
                    val text = when {
                        connected == null -> "Loading…"
                        !connected.configured -> "Discord is not connected."
                        else -> "${tr(Channels.discordStateText(connected.state))} · ${connected.bindings.orEmpty().size} ${tr("channels bound")} · ${tr("Server")} ${connected.guildId.orEmpty()}"
                    }
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(tr(text), style = Type.body, color = Palette.secondary)
                        connected?.bindings?.forEach { binding ->
                            Text("${binding.channelId} �� ${binding.allowedUsers.size} ${tr("people allowed")}",
                                style = Type.caption, color = Palette.tertiary)
                        }
                        if (connected?.configured == true) {
                            SecondaryButton(tr("Disconnect Discord"), enabled = !busy) {
                                act {
                                    discord = store.disconnectDiscord()
                                    discord = store.discordStatus() ?: discord
                                }
                            }
                        }
                    }
                }
            }
            CardSection(tr("GitHub"), footer = tr("Set up GitHub on the computer: the token stays there. Workflows can comment, label and open issues with it.")) {
                row {
                    val connected = github
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(
                            tr(when {
                                connected == null -> "Loading…"
                                !connected.configured -> "GitHub is not connected."
                                else -> "${tr("Signed in as")} ${connected.login.orEmpty()}"
                            }),
                            style = Type.body, color = Palette.secondary,
                        )
                        if (connected?.configured == true) {
                            if (connected.scopes.isNotEmpty()) {
                                Text("${tr("Scopes")}: ${connected.scopes.joinToString(", ")}", style = Type.caption, color = Palette.tertiary)
                            }
                            SecondaryButton(tr("Disconnect GitHub"), enabled = !busy) {
                                act { github = store.disconnectGitHub() }
                            }
                        }
                    }
                }
            }
            CardSection(tr("Slack")) {
                row {
                    val connected = slack
                    val text = when {
                        connected == null -> "Loading…"
                        !connected.configured -> "Slack is not connected."
                        connected.socketMode -> "Connected through Socket Mode · ${connected.workspaceId.orEmpty()}"
                        else -> "Connected through the HTTP endpoint · ${connected.workspaceId.orEmpty()}"
                    }
                    Text(tr(text), style = Type.body, color = Palette.secondary)
                }
            }
            error?.let { Text(it, style = Type.caption, color = Palette.danger) }
        }
    }
}
