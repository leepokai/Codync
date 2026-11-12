package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import kotlinx.coroutines.delay

/**
 * One endless conversation with a bot or a group, as iOS `ThreadView`: only deliberate messages
 * show; tool calls and thinking live in "Full conversation". `editGroup` opens a group's editor (its details).
 */
@Composable fun ChatScreen(state: AppState, store: AppStore, editGroup: (Bot) -> Unit) {
    val bot = state.bots.firstOrNull { it.id == state.selectedBot } ?: return
    key(state.contextId, state.generation, bot.id) { ThreadView(bot, state, store, editGroup) }
}

@Composable private fun ThreadView(bot: Bot, state: AppState, store: AppStore, editGroup: (Bot) -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Chat))
    val context = LocalContext.current
    // The bot's details sheet and the page it opens on.
    var details by remember(bot.id) { mutableStateOf<DetailsRequest?>(null) }
    val openRoutine = { id: String -> details = DetailsRequest(routineId = id) }
    // The artifact open full size, over the chat and its details.
    var viewing by remember(bot.id) { mutableStateOf<OpenArtifact?>(null) }
    // The bot whose conversation with this one is open.
    var peer by remember(bot.id) { mutableStateOf<String?>(null) }
    /** The voice call whose conversation is open (its line's entry id). */
    var callLine by remember(bot.id) { mutableStateOf<String?>(null) }
    var micDenied by remember { mutableStateOf(false) }
    var setupEntry by rememberSaveable { mutableStateOf<String?>(null) }
    var app by remember { mutableStateOf<ComposioApp?>(null) }
    var connector by remember { mutableStateOf<MarketConnector?>(null) }
    val connect = ConnectionRequestActions({ id, item -> setupEntry = id; app = item }, { id, item -> setupEntry = id; connector = item })
    val startCall = rememberStartVoice(bot.id, state, store) { micDenied = true }
    // "Talk to Codync" asked for a call here: taken even during a call, so it can't start one after it.
    LaunchedEffect(state.callRequest) { if (store.takeCallRequest(bot.id) && state.voice == null) startCall() }
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    var composerHeight by remember { mutableIntStateOf(0) }
    val density = LocalDensity.current
    // The message the composer quotes, until it sends or this chat closes.
    var replyId by rememberSaveable(bot.id) { mutableStateOf<String?>(null) }
    val replyTo = state.entries.firstOrNull { it.id == replyId && it.botId == bot.id }?.replyQuote
    val reply = { entry: Entry -> replyId = entry.id }
    // The chat ignores Full conversation over it: it stays put underneath.
    val chatState = state.copy(trace = false)
    // Find in the conversation: null while its bar is closed; each step bumps `findStep` to bring the match into view.
    var find by remember(bot.id) { mutableStateOf<ConversationFind?>(null) }
    var findStep by remember(bot.id) { mutableIntStateOf(0) }
    val matches = find?.let { ConversationFind.matches(findable(chatState.chatItems(bot)), it.query) }.orEmpty()
    val shown = find?.index(matches)?.let(matches::get)
    // The first match found is held, so earlier messages with matches loading above don't move it.
    LaunchedEffect(find?.shown == null && matches.isNotEmpty()) { find = find?.pinned(matches) }
    /** `conversation_find_searched` once per text: whether it found anything, never the text. */
    fun reportFind(found: Boolean) {
        find?.searched()?.let { find = it; Analytics.capture(AnalyticsEvent.ConversationFindSearched(found)) }
    }
    val openFind = {
        if (find == null) { Analytics.capture(AnalyticsEvent.ConversationFindOpened); find = ConversationFind() }
    }
    val stepFind = { forward: Boolean -> find = find?.step(forward, matches); findStep++; reportFind(matches.isNotEmpty()) }
    // A new text shows its first match; it counts as searched once typing pauses.
    LaunchedEffect(find?.query) {
        findStep++
        delay(700)
        reportFind(matches.isNotEmpty())
    }
    GlassStage(Modifier.fillMaxSize().background(Palette.background), chrome = {
        // Down to just above the header's bottom; the bottom one stops under Jump to latest.
        ScrollEdge(Edge.Top, top + 84.dp)
        ScrollEdge(Edge.Bottom, with(density) { composerHeight.toDp() } + 4.dp, Modifier.align(Alignment.BottomCenter))
        ThreadHeader(bot, state, store, openFind) { if (bot.kind == "group") editGroup(bot) else details = DetailsRequest() }
        // The call and the find bar float over the chat under the header, as on iOS; the chat stays readable and usable.
        Column(Modifier.padding(top = top + 54.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            AnimatedVisibility(state.voice != null,
                enter = fadeIn(Motion.fade()) + slideInVertically(Motion.layoutOffset) { -it }, exit = fadeOut(Motion.fade()) + slideOutVertically(Motion.layoutOffset) { -it }) {
                state.voice?.let { CallBar(it, state, store) }
            }
            AnimatedVisibility(find != null,
                enter = fadeIn(Motion.fade()) + slideInVertically(Motion.layoutOffset) { -it }, exit = fadeOut(Motion.fade()) + slideOutVertically(Motion.layoutOffset) { -it }) {
                find?.let { current ->
                    // Closing clears the highlight with the bar.
                    FindBar(current, matches, { find = current.type(it) }, stepFind) { reportFind(matches.isNotEmpty()); find = null }
                }
            }
        }
        // Measured with the keyboard and navigation bar under it, so the last message clears them too.
        Column(Modifier.align(Alignment.BottomCenter).fillMaxWidth().onSizeChanged { composerHeight = it.height }
            .imePadding().navigationBarsPadding()) {
            // Nothing sent now could be read on the other side.
            val mismatch = state.mismatch
            if (mismatch != null) UpdateNeededCard(state, mismatch, store::requestHostUpdate, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp))
            // An artifact's viewer has its own box for the same draft; two would overwrite each other.
            else if (viewing == null) MessageComposer(bot, chatState, store, onCall = if (state.voice == null && state.actualConnection is LinkState.Ready) startCall else null,
                onInterrupt = if (state.voice?.botId == bot.id && state.voice.state.phase == VoicePhase.Speaking) store::interruptVoice else null,
                replyTo = replyTo, cancelReply = { replyId = null },
                queued = queuedMessages(state.chatEntries(bot), bot.isWorking))
        }
    }) {
        Transcript(bot, chatState, store, connect, PaddingValues(start = 16.dp, end = 16.dp, top = top + 60.dp,
            bottom = with(density) { composerHeight.toDp() } + 8.dp), indicatorTop = top + 50.dp, openRoutine = openRoutine, reply = reply,
            addFallback = { details = DetailsRequest(botId = it.id) }, openArtifact = { viewing = it }, openCall = { callLine = it },
            found = shown?.id, findStep = findStep) { peer = it }
    }
    CodyncDialog("Microphone access is off", micDenied, { micDenied = false },
        message = "Allow the microphone in Android settings to talk to your bot.") {
        listOf(DialogAction("Open settings") { openAppSettings(context) })
    }
    CodyncSheet(state.trace, { store.showTrace(false) }) {
        AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Trace))
        TraceView(bot, state)
    }
    CodyncSheet(details, { details = null }) { request ->
        val shown = state.bots.firstOrNull { it.id == (request.botId ?: bot.id) }
        if (shown != null) BotDetailsView(shown, request, state, store) { viewing = it }
    }
    ArtifactLayer(viewing) { open ->
        state.bots.firstOrNull { it.id == open.botId }?.let { owner -> ArtifactViewer(open, owner, state, store) { viewing = null } }
    }
    CodyncSheet(peer, { peer = null }) { BotConversationView(bot, it, state, store) }
    CodyncSheet(callLine, { callLine = null }) { CallConversationView(bot.id, it, state, store) }
    CodyncSheet(app, { app = null; setupEntry = null }) { ComposioConnectSheet(it, state, store, setupEntry) { store.refreshPlugins() } }
    CodyncSheet(connector, { connector = null; setupEntry = null }) { InstallConnectorSheet(it, state, store, setupEntry) }
}

/** The glass back button, the title (tapping it opens the bot's details) and the connection under it. */
@Composable private fun ThreadHeader(bot: Bot, state: AppState, store: AppStore, openFind: () -> Unit, openDetails: () -> Unit) {
    Box(Modifier.fillMaxWidth().statusBarsPadding().padding(top = 6.dp).padding(horizontal = 16.dp).height(44.dp)) {
        val (source, pressed) = rememberPress()
        Box(Modifier.align(Alignment.CenterStart).size(44.dp).glass(Capsule(), pressed = pressed)
            .then(Modifier.plainButton { store.openBot(null) }).semantics { contentDescription = "Back" }, contentAlignment = Alignment.Center) {
            Symbol("chevron.left", size = 32.dp, tint = Palette.text)
        }
        val label = tr("{name}, view details").replace("{name}", bot.name)
        Box(Modifier.align(Alignment.Center).plainButton(action = openDetails).semantics { contentDescription = label }) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                    BotAvatar(bot, state.bots, 22.dp)
                    Text(bot.name, style = Type.system(13f).semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                ConnectionSubtitle(state, devServerSummary(bot.devServers))
            }
        }
        Row(Modifier.align(Alignment.CenterEnd), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            HeaderButton("magnifyingglass", "Find in conversation", action = openFind)
            // The corner is the computer: it pulses while this bot is operating it (watch, then take over).
            val screen = state.screen
            if (screen != null) {
                val operating = screen.agentBot == bot.id
                ComputerButton(operating, Modifier) { store.openScreen(bot.id.takeIf { operating }) }
            }
        }
    }
}

/** A glass button in the header's corner. */
@Composable private fun HeaderButton(symbol: String, title: String, modifier: Modifier = Modifier, symbolModifier: Modifier = Modifier,
    action: () -> Unit) {
    val (source, pressed) = rememberPress()
    val label = tr(title)
    Box(modifier.size(44.dp).glass(Capsule(), pressed = pressed)
        .clickable(source, null, role = Role.Button, onClick = action).semantics { contentDescription = label },
        contentAlignment = Alignment.Center) {
        Symbol(symbol, symbolModifier, size = 22.dp, tint = Palette.text)
    }
}

/** Find in the open conversation (iOS `FindBar`): the text, where the shown match stands, previous / next and close. */
@Composable private fun FindBar(find: ConversationFind, matches: List<ConversationFind.Match>, type: (String) -> Unit,
    step: (Boolean) -> Unit, close: () -> Unit) {
    Row(Modifier.padding(horizontal = 12.dp).fillMaxWidth().glass(Capsule()).padding(start = 6.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        SearchField(find.query, type, Modifier.weight(1f), "Find in conversation", autoFocus = true) { step(true) }
        val label = find.label(matches, tr("No results"))
        if (label.isNotEmpty()) Text(label, Modifier.padding(horizontal = 4.dp), style = Type.footnote.monospacedDigit,
            color = if (matches.isEmpty()) Palette.attention else Palette.secondary, maxLines = 1)
        IconButton("Previous match", "chevron.up", enabled = matches.isNotEmpty()) { step(false) }
        IconButton("Next match", "chevron.down", enabled = matches.isNotEmpty()) { step(true) }
        IconButton("Close", "xmark", action = close)
    }
}

@Composable private fun ComputerButton(operating: Boolean, modifier: Modifier, action: () -> Unit) {
    val pulse by rememberInfiniteTransition(label = "Operating").animateFloat(1f, 0.35f,
        infiniteRepeatable(tween(900), RepeatMode.Reverse), label = "Pulse")
    HeaderButton("desktopcomputer", "Computer", modifier, Modifier.graphicsLayer { alpha = if (operating) pulse else 1f }, action)
}

/** The link to the computer: route icon (or a spinner) and a word, then what the bot keeps running. */
@Composable private fun ConnectionSubtitle(state: AppState, summary: DevServerSummary) {
    val servers = when (summary) {
        DevServerSummary.None -> null
        DevServerSummary.Starting -> tr("Starting")
        DevServerSummary.Failed -> tr("Failed to start")
        is DevServerSummary.Running -> summary.text
    }
    val style = Type.system(10f).medium
    val description = listOfNotNull(state.hostName, connectionDescription(state), servers).joinToString(", ")
    Row(Modifier.clearAndSetSemantics { contentDescription = description }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Box(Modifier.width(12.dp), contentAlignment = Alignment.Center) {
            when (val link = state.connection) {
                LinkState.Connecting -> Spinner(8.dp)
                is LinkState.Ready -> Symbol(if (link.route == HostRoute.Relay) "cloud" else "wifi", size = 11.dp, tint = Palette.secondary)
                else -> Symbol("wifi.slash", size = 11.dp, tint = Palette.secondary)
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(connectionLabel(state), style = style, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (servers != null) {
                Text("·", style = style, color = Palette.secondary)
                if (summary == DevServerSummary.Starting) Spinner(8.dp)
                Text(servers, style = style.monospacedDigit, color = if (summary == DevServerSummary.Failed) Palette.danger else Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/** [bot]'s chat. What voice calls carried shows in each call's conversation instead. */
private fun AppState.chatEntries(bot: Bot) = entries.filter { it.botId == bot.id && it.isChat }.hidingCalls()

/** [bot]'s rows. A call's message that failed to send stays here, to be sent again; messages waiting behind the bot's
 *  work show above the composer instead. */
private fun AppState.chatItems(bot: Bot) =
    chatItems(chatEntries(bot), pending.filter { it.botId == bot.id && (it.call == null || it.status == SendStatus.Failed) }, busy = bot.isWorking)

/** The messages: all this phone has, earlier ones from the computer near the top, following the newest. */
@Composable private fun Transcript(bot: Bot, state: AppState, store: AppStore, connect: ConnectionRequestActions, padding: PaddingValues,
    indicatorTop: Dp, openRoutine: (String) -> Unit, reply: (Entry) -> Unit, addFallback: (Bot) -> Unit, openArtifact: (OpenArtifact) -> Unit,
    openCall: (String) -> Unit, found: String?, findStep: Int, openPeer: (String) -> Unit) {
    val entries = state.chatEntries(bot)
    val items = state.chatItems(bot)
    val working = bot.isWorking(bot.id) && state.connection is LinkState.Ready
    // The ceiling counts rows the chat shows. Tool and thought rows stay in memory for Trace, but they do not fill it.
    val shown = entries.size
    val floor = state.entries.filter { it.botId == bot.id }.minOfOrNull { it.seq } ?: Long.MAX_VALUE
    val more = Residency.needsEarlier(shown, bot.id in state.historyEnd)
    val details by ThinkingDetails.enabled.collectAsStateWithLifecycle()
    val list = rememberLazyListState()
    var settled by remember { mutableStateOf(false) }
    val paging = remember { EarlierPaging() }
    // Pull to refresh reconnects, like Reconnect in the roster's computer menu, until the link is back.
    // Find's shown match: its row, after the earlier-messages row when there is one.
    val reveal = items.indexOfFirst { it.id == found }.takeIf { it >= 0 }?.let { ConversationReveal(items[it].id, it + if (more) 1 else 0, findStep) }
    ConversationList(list, items.lastOrNull()?.id, entries.lastOrNull()?.rev, working, padding, store::refreshConnection, indicatorTop,
        reveal = reveal, onOpened = { settled = true }) {
        if (more) item("earlier") {
            Box(Modifier.fillMaxWidth().padding(vertical = 14.dp).clearAndSetSemantics { contentDescription = "Loading earlier messages" },
                contentAlignment = Alignment.Center) { Spinner(14.dp) }
            // The list opens on the newest message, and this row is composed on that first frame. Page only after that
            // scroll has finished and the row is still showing: a short chat never scrolls, so the row staying up is the trigger.
            val loading = historyKey(bot.id) in state.busy
            val visible by remember { derivedStateOf { list.layoutInfo.visibleItemsInfo.any { it.key == "earlier" } } }
            LaunchedEffect(shown, floor, loading, settled, visible) {
                if (paging.ask(settled, visible, loading, shown, floor)) store.loadHistory(paging::failed)
            }
            // Scrolled far away, the list drops the row before it recomposes as hidden.
            DisposableEffect(paging) { onDispose(paging::left) }
        }
        // Shown while earlier messages may still load, as on the iPhone.
        if (items.isEmpty()) item("intro") { IntroCard(bot, state) }
        items(items, key = { it.id }) { item ->
            // The message holding find's shown match is highlighted.
            Box(Modifier.background(if (item.id == found) Palette.attention.copy(alpha = 0.18f) else Color.Transparent)) {
            when (item) {
                is ChatItem.Separator -> TimeSeparator(item.at)
                is ChatItem.Message -> {
                    val routine = item.entry.data.text("routineId").takeIf { item.entry.kind == "notice" }
                    if (routine != null) RoutineNotice(item.entry, retrying = "retry:" + item.entry.id in state.busy,
                        retry = { store.retryRoutineRun(item.entry) }) { openRoutine(routine) }
                    else if (item.entry.kind == "notice" && item.entry.call != null) Box(Modifier.padding(top = 6.dp)) {
                        CallLineRow(item.entry) { openCall(item.entry.id) }
                    }
                    else ChatRow(item.entry, item.groupStart, bot, state, store, openTrace = { store.showTrace(true) },
                        // An update card takes the composer's place, so there's nothing to reply with.
                        reply = if (item.entry.isReplyable && state.mismatch == null) { { reply(item.entry) } } else null, connect = connect,
                        addFallback = addFallback, openArtifact = openArtifact)
                }
                is ChatItem.Pending -> PendingBubble(item.send, item.groupStart, state, store)
                is ChatItem.Exchanges -> BotMessageRow(item.group, state) { openPeer(item.group.peerId) }
            }
            }
        }
        if (working) item("working") { WorkingIndicator(bot, workingSteps(state.entries.filter { it.botId == bot.id }), details) }
    }
}
