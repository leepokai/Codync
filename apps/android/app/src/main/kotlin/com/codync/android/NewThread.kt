package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.statusBars
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.LinkState
import com.codync.android.core.Login
import com.codync.android.core.fill
import com.codync.android.design.*

// A thread the user starts from the Threads tab (the desktop's `ThreadStart`): a chat nothing was said in yet, pushed
// over the list, with who it's with in the middle and the chat's box under it, the bot and model chips in its row. It
// opens on no bot, on the first computer the tab shows that starts one so, else on the lead bot. A bot's thread runs on
// that bot's own model, approvals and folder; one without a bot on the agent and model its chip picks (the model panel),
// in a folder of its own (an older host: the computer's home folder), asking before anything it does, in the agent's
// Auto mode. Text only: the phones don't send files with a new thread.

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

/** The draft a new thread is written in, one per computer (it stays when the bot changes). */
private const val NEW_THREAD_DRAFT = "thread/new"

/**
 * The page a new thread starts from. Sent, [opened] puts the new thread's chat in its place (null: it had no thread in
 * time; a bot's details open instead). A failure shows under the box and keeps the text.
 */
@Composable internal fun NewThreadPage(bots: List<WorkingBot>, plain: AppState?, accounts: AccountStore, back: () -> Unit, opened: (ThreadRef?) -> Unit) {
    // The bot it's with, by computer and id; null for no bot.
    var pick by remember { mutableStateOf(if (plain == null) (bots.firstOrNull { it.bot.lead } ?: bots.firstOrNull())?.let { it.state.computer.id to it.bot.id } else null) }
    // Picked again when the computer that starts threads without a bot comes (or changes) while it's open.
    val initial = defaultLogin(plain)
    var login by remember(plain?.computer?.id) { mutableStateOf(initial) }
    var model by remember(plain?.computer?.id) { mutableStateOf<String?>(null) }
    var options by remember(plain?.computer?.id) { mutableStateOf(emptyMap<String, String>()) }
    var choosingModel by remember { mutableStateOf(false) }
    var failure by remember { mutableStateOf<String?>(null) }
    // Null for no bot, or for one that went away with its computer.
    val bot = pick?.let { (computer, id) -> bots.firstOrNull { it.state.computer.id == computer && it.bot.id == id } }
    val target = if (pick == null) plain?.takeIf { login.backend.isNotEmpty() } else bot?.state
    val store = target?.let { accounts.store(it.computer.id) }
    // The computer whose box it writes in: the one it starts on, else any it could.
    val shown = target ?: plain ?: bots.firstOrNull()?.state
    val shownStore = shown?.let { accounts.store(it.computer.id) }
    // Nothing left to start a thread with.
    LaunchedEffect(shownStore == null) { if (shownStore == null) back() }
    if (shown == null || shownStore == null) return
    val sending = target != null && startThreadKey(bot?.bot?.id) in target.busy
    var composerHeight by remember { mutableIntStateOf(0) }
    var left by remember { mutableStateOf(false) }
    DisposableEffect(Unit) { onDispose { left = true } }
    val density = LocalDensity.current
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    // A thread chat's page: glass Back over the stage, its top edge soft as the chat's.
    GlassStage(Modifier.fillMaxSize().background(Palette.background), chrome = {
            ScrollEdge(Edge.Top, top + 56.dp)
            Box(Modifier.padding(top = top + 6.dp, start = 16.dp)) { GlassBackButton(back) }
            Column(Modifier.align(Alignment.BottomCenter).fillMaxWidth().onSizeChanged { composerHeight = it.height }.imePadding().navigationBarsPadding()) {
                key(shown.computer.id) {
                    ThreadComposer(shown, shownStore, NEW_THREAD_DRAFT, takesFiles = false, placeholder = tr("What should it work on?"),
                        ready = store != null && !sending, accessory = {
                            BotChip(bots, bot, pick == null, plain != null, Modifier.weight(1f, fill = false)) { pick = it }
                            if (pick == null && plain != null)
                                ModelChipLabel(agentRegistry(plain, login.backend), model, Modifier.weight(1f, fill = false)) { choosingModel = true }
                        }) { message, _ ->
                        val starting = store ?: return@ThreadComposer
                        failure = null
                        val setup = if (pick == null && plain != null) plainSetup(login, if (plain.threadFolders) "" else plain.home, model, options) else null
                        starting.startThread(bot?.bot?.id, message, setup, failed = { failure = it }) { ref ->
                            // Left meanwhile: a draft written since on a page opened again stays, and nothing opens.
                            if (starting.state.value.drafts[NEW_THREAD_DRAFT]?.text == message) starting.editDraft(NEW_THREAD_DRAFT, "")
                            if (!left) opened(ref)
                        }
                    }
                }
                failure?.let { Text(tr(it), Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, bottom = 12.dp), style = Type.caption, color = Palette.danger) }
            }
        }) {
            NewThreadIntro(bot, plain?.takeIf { pick == null }, login,
                Modifier.fillMaxSize().padding(start = 24.dp, end = 24.dp, top = top + 50.dp, bottom = with(density) { composerHeight.toDp() }))
        }
    val plainStore = plain?.let { accounts.store(it.computer.id) }
    if (plain != null && plainStore != null) CodyncSheet(choosingModel, { choosingModel = false }) {
        ModelPanel(plain, plainStore, backend = login.backend, loginProfile = login.loginProfile, model = model, options = options,
            choose = { m, o -> model = m; options = o }, fast = null, setFast = null, bot = null, seats = readySeats(plain, login),
            pickSeat = { login = it; model = null; options = emptyMap() })
    }
}

/** Who it's with, in the page's middle: [bot], or without one the agent [login] runs on [plain]. */
@Composable private fun NewThreadIntro(bot: WorkingBot?, plain: AppState?, login: Login, modifier: Modifier) {
    val name = bot?.bot?.name ?: plain?.let { backendName(it, login.backend) }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically), horizontalAlignment = Alignment.CenterHorizontally) {
        if (bot != null) BotAvatar(bot.bot, bot.state.bots, 72.dp)
        else if (plain != null) AgentIcon(agentRegistry(plain, login.backend), 56.dp)
        if (name != null) Text(fill(tr("New thread with {name}"), "name" to name), Modifier.semantics { heading() },
            style = Type.title2.semibold, color = Palette.text, textAlign = TextAlign.Center)
    }
}

/** Who the thread is with, in the box's row: [bots], then No bot where a computer can ([plainOn]); [choose] takes a bot by computer and id, or null. */
@Composable private fun BotChip(bots: List<WorkingBot>, bot: WorkingBot?, noBot: Boolean, plainOn: Boolean, modifier: Modifier, choose: (Pair<String, String>?) -> Unit) {
    val several = bots.distinctBy { it.state.computer.id }.size > 1
    DropdownMenu({
        bots.map { of -> MenuItem(of.bot.name, verbatim = true, detail = of.state.hostName.takeIf { several },
            selected = of === bot) { choose(of.state.computer.id to of.bot.id) } } +
            listOfNotNull(MenuItem("No bot", icon = "app.dashed", selected = noBot, divider = bots.isNotEmpty()) { choose(null) }.takeIf { plainOn })
    }, modifier, label = tr("Choose a bot")) {
        ChipLabel {
            if (bot != null) {
                BotAvatar(bot.bot, bot.state.bots, 18.dp, animated = false)
                Text(bot.bot.name, style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            } else if (noBot) {
                Symbol("app.dashed", size = 18.dp, tint = Palette.secondary)
                Text(tr("No bot"), style = Type.subheadline, color = Palette.secondary)
            } else Text(tr("Choose a bot"), style = Type.subheadline, color = Palette.secondary)
        }
    }
}
