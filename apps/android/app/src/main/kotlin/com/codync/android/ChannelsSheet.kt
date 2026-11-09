package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import com.codync.android.core.DiscordBinding
import com.codync.android.core.DiscordStatus
import com.codync.android.core.GitHub
import com.codync.android.core.GitHubStatus
import com.codync.android.core.SlackStatus
import com.codync.android.core.TelegramBot
import com.codync.android.design.*
import kotlinx.coroutines.launch

/**
 * The messaging channels of the computer: its Telegram bots, Discord and Slack connections, and the GitHub token
 * workflows use. The host holds every token; a phone sets them up, pairs people to a bot and takes them out, exactly
 * as the computer's own settings do.
 */
@Composable internal fun ChannelsSheet(state: AppState, store: AppStore) {
    var bots by remember { mutableStateOf<List<TelegramBot>?>(null) }
    var slack by remember { mutableStateOf<SlackStatus?>(null) }
    var discord by remember { mutableStateOf<DiscordStatus?>(null) }
    var github by remember { mutableStateOf<GitHubStatus?>(null) }
    var pairing by remember { mutableStateOf<Pair<String, String>?>(null) }
    var removing by remember { mutableStateOf<String?>(null) }
    var adding by remember { mutableStateOf(false) }
    var newName by remember { mutableStateOf("") }
    var newToken by remember { mutableStateOf("") }
    var newBot by remember { mutableStateOf("") }
    var editingDiscord by remember { mutableStateOf(false) }
    var guildId by remember { mutableStateOf("") }
    var botUserId by remember { mutableStateOf("") }
    var discordToken by remember { mutableStateOf("") }
    var rows by remember { mutableStateOf(listOf<BindingDraft>()) }
    var githubToken by remember { mutableStateOf("") }
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
    // The bots a channel may answer with: not a group, not a terminal session.
    val choices = state.bots.filter { it.kind != "group" && it.kind != "external" }
    val fallbackBot = choices.firstOrNull()?.id.orEmpty()
    // One change at a time; the host's answer replaces what is shown.
    fun act(work: suspend () -> Unit) {
        if (busy) return
        busy = true; error = null
        scope.launch {
            try { work() } catch (e: Exception) { error = e.message ?: "That did not work." } finally { busy = false }
        }
    }
    fun startDiscord() {
        val current = discord
        guildId = current?.guildId.orEmpty()
        botUserId = current?.botUserId.orEmpty()
        discordToken = ""
        rows = current?.bindings.orEmpty().map { BindingDraft(it.channelId, it.botId, it.allowedUsers.joinToString(", ")) }
            .ifEmpty { listOf(BindingDraft("", "", "")) }
        editingDiscord = true
    }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Channels")
        CardForm {
            val list = bots
            CardSection(tr("Telegram"), footer = tr("Add a Telegram bot, or remove one, here or on the computer: its token stays there.")) {
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
                            SecondaryButton(tr("Remove bot"), enabled = !busy) { removing = bot.name }
                        }
                    }
                }
                if (adding) {
                    row {
                        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(tr("Telegram bot name"), style = Type.caption, color = Palette.secondary)
                            PlainTextField(newName, { newName = it }, prompt = tr("A short name: home, work…"), singleLine = true)
                            Text(tr("Telegram bot token"), style = Type.caption, color = Palette.secondary)
                            PlainTextField(newToken, { newToken = it }, prompt = tr("The token @BotFather gave you"), singleLine = true)
                            ChoicePicker(newBot.ifEmpty { fallbackBot }, choices.map { it.id to it.name }, { newBot = it })
                            SecondaryButton(if (busy) tr("Adding…") else tr("Add Telegram bot"),
                                enabled = !busy && Channels.nameOk(newName.trim()) && Channels.tokenOk(newToken.trim()) && choices.isNotEmpty()) {
                                act {
                                    val botId = newBot.ifEmpty { fallbackBot }
                                    bots = store.addTelegramBot(newName, newToken, botId)
                                    newName = ""; newToken = ""; newBot = ""; adding = false
                                }
                            }
                        }
                    }
                } else {
                    row { SecondaryButton(tr("Add a bot"), enabled = !busy && list != null) { adding = true } }
                }
            }
            CardSection(tr("Discord"), footer = tr("Make a Discord app, add its bot to the server, and paste the bot token here. In the app, turn on the Message Content intent. Only the people allowed in each channel can write to it.")) {
                val connected = discord
                if (connected?.configured == true && !editingDiscord) {
                    row {
                        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text("${tr(Channels.discordStateText(connected.state))} · ${connected.bindings.orEmpty().size} ${tr("channels bound")} · ${tr("Server")} ${connected.guildId.orEmpty()}",
                                style = Type.body, color = Palette.secondary)
                            connected.bindings?.forEach { binding ->
                                Text("${binding.channelId} · ${binding.allowedUsers.size} ${tr("people allowed")}",
                                    style = Type.caption, color = Palette.tertiary)
                            }
                            SecondaryButton(tr("Change Discord settings"), enabled = !busy) { startDiscord() }
                            SecondaryButton(tr("Disconnect Discord"), enabled = !busy) {
                                act {
                                    discord = store.disconnectDiscord()
                                    discord = store.discordStatus() ?: discord
                                }
                            }
                        }
                    }
                } else if (connected == null && !editingDiscord) {
                    row { Text(tr("Loading…"), style = Type.body, color = Palette.secondary) }
                } else {
                    if (editingDiscord && rows.isEmpty()) startDiscord()
                    row {
                        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(tr("Server id"), style = Type.caption, color = Palette.secondary)
                            PlainTextField(guildId, { guildId = it }, prompt = "123456789012345678", singleLine = true)
                            Text(tr("Bot user id"), style = Type.caption, color = Palette.secondary)
                            PlainTextField(botUserId, { botUserId = it }, prompt = "123456789012345678", singleLine = true)
                            Text(tr("Bot token"), style = Type.caption, color = Palette.secondary)
                            PlainTextField(discordToken, { discordToken = it }, prompt = tr("The token from the Bot tab"), singleLine = true)
                            for ((index, draft) in rows.withIndex()) {
                                Text(tr("Channel id"), style = Type.caption, color = Palette.secondary)
                                PlainTextField(draft.channelId, { rows = rows.updated(index, draft.copy(channelId = it)) },
                                    prompt = "123456789012345678", singleLine = true)
                                ChoicePicker(draft.botId.ifEmpty { fallbackBot }, choices.map { it.id to it.name },
                                    { rows = rows.updated(index, draft.copy(botId = it)) })
                                Text(tr("Allowed user ids"), style = Type.caption, color = Palette.secondary)
                                PlainTextField(draft.allowed, { rows = rows.updated(index, draft.copy(allowed = it)) },
                                    prompt = tr("Separated by spaces or commas"), singleLine = true)
                                if (rows.size > 1) SecondaryButton(tr("Remove channel")) { rows = rows.filterIndexed { i, _ -> i != index } }
                            }
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                SecondaryButton(tr("Add channel")) { rows = rows + BindingDraft("", "", "") }
                                val clean = rows.map {
                                    DiscordBinding(it.channelId.trim(), it.botId.ifEmpty { fallbackBot },
                                        it.allowed.split(Regex("[\\s,]+")).filter { u -> u.isNotEmpty() })
                                }.filter { it.channelId.isNotEmpty() || it.allowedUsers.isNotEmpty() }
                                val ready = Channels.idOk(guildId.trim()) && Channels.idOk(botUserId.trim()) &&
                                    Channels.discordTokenOk(discordToken.trim()) && clean.isNotEmpty() &&
                                    clean.all { Channels.idOk(it.channelId) && it.botId.isNotEmpty() && it.allowedUsers.isNotEmpty() }
                                SecondaryButton(if (busy) tr("Checking…") else tr("Connect Discord"), enabled = !busy && ready) {
                                    act {
                                        discord = store.setDiscord(guildId, botUserId, discordToken, clean)
                                        editingDiscord = false; discordToken = ""
                                    }
                                }
                                if (connected?.configured == true) {
                                    SecondaryButton(tr("Cancel"), enabled = !busy) { editingDiscord = false }
                                }
                            }
                        }
                    }
                }
            }
            CardSection(tr("GitHub"), footer = tr("A fine-grained token with the repository permissions your workflows need. It stays on the computer; Codync never sends it anywhere else.")) {
                val connected = github
                row {
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
                        } else if (connected != null) {
                            PlainTextField(githubToken, { githubToken = it }, prompt = "github_pat_…", singleLine = true)
                            SecondaryButton(if (busy) tr("Checking…") else tr("Connect GitHub"),
                                enabled = !busy && GitHub.tokenOk(githubToken)) {
                                act {
                                    github = store.setGitHubToken(githubToken)
                                    githubToken = ""
                                }
                            }
                        }
                    }
                }
            }
            CardSection(tr("Slack"), footer = tr("Slack is set up with the host's setSlackChannel call; see the Slack checklist.")) {
                val connected = slack
                row {
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        val text = when {
                            connected == null -> tr("Loading…")
                            !connected.configured -> tr("Slack is not connected.")
                            connected.socketMode -> "${tr("Connected through Socket Mode")} · ${connected.workspaceId.orEmpty()}"
                            else -> "${tr("Connected through the HTTP endpoint")} · ${connected.workspaceId.orEmpty()}"
                        }
                        Text(text, style = Type.body, color = Palette.secondary)
                        if (connected?.configured == true) {
                            SecondaryButton(tr("Disconnect Slack"), enabled = !busy) {
                                act { slack = store.disconnectSlack() }
                            }
                        }
                    }
                }
            }
            error?.let { Text(it, style = Type.caption, color = Palette.danger) }
        }
    }
    CodyncDialog(tr("Remove this Telegram bot?"), removing != null, { removing = null },
        message = tr("Its token is deleted from the computer and the people paired to it can no longer write to your bot."),
        actions = { listOf(DialogAction("Remove", destructive = true) { val name = removing; removing = null; if (name != null) act { bots = store.removeTelegramBot(name) } }) })
}

/** One channel the Discord form is filling in. */
private data class BindingDraft(val channelId: String, val botId: String, val allowed: String)

private fun List<BindingDraft>.updated(index: Int, draft: BindingDraft): List<BindingDraft> =
    mapIndexed { i, row -> if (i == index) draft else row }
