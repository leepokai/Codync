package com.codync.android

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.LinkState
import com.codync.android.core.Login
import com.codync.android.design.*
import com.kyant.shapes.Capsule

// A thread the user starts from the Threads tab (the desktop's New thread): its first message, then who it's with under
// the field. It opens on no bot, on the first computer the tab shows that starts one so, else on the lead bot. A bot's
// thread runs on that bot's own model, approvals and folder; one without a bot on the agent and model its chip picks (the
// model panel), in the computer's home folder, asking before anything it does, in the agent's Auto mode.

/** The bots a thread can start with now (desktop `threadBots`): [roster]'s own bots, no group chats, on computers online that start threads. */
internal fun threadBots(roster: List<WorkingBot>): List<WorkingBot> = roster.filter { of ->
    !of.bot.plain && of.bot.kind != "group" && of.state.actualConnection is LinkState.Ready && of.state.startsThreads
}

/** The first of [shown] where a thread may start without a bot (desktop `plainStores`): online, with an agent ready. */
@Composable internal fun plainComputer(shown: List<AppState>): AppState? =
    shown.firstOrNull { it.actualConnection is LinkState.Ready && it.startsThreadsWithoutBot && readySeats(it, Login("")).isNotEmpty() }

/** Claude Code where it's ready on [plain], else the first agent that is (the desktop's); no agent without one. */
@Composable private fun defaultLogin(plain: AppState?): Login {
    val seats = plain?.let { readySeats(it, Login("")) }.orEmpty()
    return (seats.firstOrNull { it.login == Login("claude") } ?: seats.firstOrNull())?.login ?: Login("")
}

/** The sheet a new thread starts from; [open] shows it. Send opens the thread once it starts ([openLoose] for one without a bot). */
@Composable internal fun NewThreadSheet(open: Boolean, bots: List<WorkingBot>, plain: AppState?, accounts: AccountStore, dismiss: () -> Unit,
    openLoose: (LooseThread) -> Unit) {
    CodyncSheet(open, dismiss, fitted = true) { NewThreadForm(bots, plain, accounts, dismiss, openLoose) }
}

@Composable private fun NewThreadForm(bots: List<WorkingBot>, plain: AppState?, accounts: AccountStore, dismiss: () -> Unit, openLoose: (LooseThread) -> Unit) {
    var text by remember { mutableStateOf("") }
    // The bot it's with, by computer and id; null for no bot.
    var with by remember { mutableStateOf(if (plain == null) (bots.firstOrNull { it.bot.lead } ?: bots.firstOrNull())?.let { it.state.computer.id to it.bot.id } else null) }
    // Picked again when the computer that starts threads without a bot comes (or changes) while it's open.
    val initial = defaultLogin(plain)
    var login by remember(plain?.computer?.id) { mutableStateOf(initial) }
    var model by remember(plain?.computer?.id) { mutableStateOf<String?>(null) }
    var options by remember(plain?.computer?.id) { mutableStateOf(emptyMap<String, String>()) }
    var choosingModel by remember { mutableStateOf(false) }
    // Null for no bot, or for one that went away with its computer.
    val bot = with?.let { (computer, id) -> bots.firstOrNull { it.state.computer.id == computer && it.bot.id == id } }
    val target = if (with == null) plain?.takeIf { login.backend.isNotEmpty() } else bot?.state
    val store = target?.let { accounts.store(it.computer.id) }
    val sending = store != null && startThreadKey(bot?.bot?.id) in (store.state.collectAsState().value.busy)
    Column {
        ModalHeader({ Text(tr("New thread"), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis) }) {
            AnimatedContent(sending, transitionSpec = { fadeIn(Motion.fade()) togetherWith fadeOut(Motion.fade()) }, label = "Send") { busy ->
                if (busy) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner(18.dp) }
                // A failure shows as every action's does and keeps the message here.
                else IconButton("Send", "arrow.up", enabled = text.isNotBlank() && store != null) {
                    val setup = if (with == null && plain != null) plainSetup(login, plain.home, model, options) else null
                    store?.startThread(bot?.bot?.id, text, setup, dismiss, openLoose)
                }
            }
        }
        CardSection(modifier = Modifier.padding(start = 20.dp, end = 20.dp)) {
            row { PlainTextField(text, { text = it }, Modifier.fillMaxWidth(), "What should it work on?", minLines = 3, maxLines = 8) }
        }
        Row(Modifier.padding(start = 12.dp, end = 20.dp, top = 6.dp, bottom = 20.dp), horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalAlignment = Alignment.CenterVertically) {
            val several = bots.distinctBy { it.state.computer.id }.size > 1
            DropdownMenu({
                bots.map { of -> MenuItem(of.bot.name, verbatim = true, detail = of.state.hostName.takeIf { several },
                    selected = of === bot) { with = of.state.computer.id to of.bot.id } } +
                    listOfNotNull(MenuItem("No bot", icon = "person.crop.circle.dashed", selected = with == null, divider = bots.isNotEmpty()) { with = null }
                        .takeIf { plain != null })
            }, label = tr("Choose a bot")) {
                ChipLabel {
                    if (bot != null) {
                        BotAvatar(bot.bot, bot.state.bots, 18.dp, animated = false)
                        Text(bot.bot.name, style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    } else if (with == null) {
                        Symbol("person.crop.circle.dashed", size = 16.dp, tint = Palette.secondary)
                        Text(tr("No bot"), style = Type.subheadline, color = Palette.secondary)
                    } else Text(tr("Choose a bot"), style = Type.subheadline, color = Palette.secondary)
                }
            }
            if (with == null && plain != null) Box(Modifier.clip(Capsule()).clickable(role = Role.Button) { choosingModel = true }
                .semantics { contentDescription = "Model" }) {
                ChipLabel {
                    AgentIcon(agentRegistry(plain, login.backend), 16.dp)
                    Text(model ?: tr("Default"), style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
    val plainStore = plain?.let { accounts.store(it.computer.id) }
    if (plain != null && plainStore != null) CodyncSheet(choosingModel, { choosingModel = false }) {
        Column(Modifier.fillMaxSize()) {
            ModalHeader("Model")
            ModelPanel(plain, plainStore, backend = login.backend, loginProfile = login.loginProfile, model = model, options = options,
                choose = { m, o -> model = m; options = o }, fast = null, setFast = null, bot = null, seats = readySeats(plain, login),
                pickSeat = { login = it; model = null; options = emptyMap() })
        }
    }
}
