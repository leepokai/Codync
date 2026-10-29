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
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
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
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    var composerHeight by remember { mutableIntStateOf(0) }
    val density = LocalDensity.current
    // The message the composer quotes, until it sends or this chat closes.
    var replyId by rememberSaveable(bot.id) { mutableStateOf<String?>(null) }
    val replyTo = state.entries.firstOrNull { it.id == replyId && it.botId == bot.id }?.replyQuote
    val reply = { entry: Entry -> replyId = entry.id }
    // The chat ignores Full conversation over it: it stays put underneath.
    val chatState = state.copy(trace = false)
    GlassStage(Modifier.fillMaxSize().background(Palette.background), chrome = {
        // Down to just above the header's bottom; the bottom one stops under Jump to latest.
        ScrollEdge(Edge.Top, top + 84.dp)
        ScrollEdge(Edge.Bottom, with(density) { composerHeight.toDp() } + 4.dp, Modifier.align(Alignment.BottomCenter))
        ThreadHeader(bot, state, store) { if (bot.kind == "group") editGroup(bot) else details = DetailsRequest() }
        // The call floats over the chat under the header, as on iOS; the chat stays readable and usable.
        AnimatedVisibility(state.voice != null, Modifier.padding(top = top + 54.dp),
            enter = fadeIn(Motion.fade()) + slideInVertically(Motion.layoutOffset) { -it }, exit = fadeOut(Motion.fade()) + slideOutVertically(Motion.layoutOffset) { -it }) {
            state.voice?.let { CallBar(it, state, store) }
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
                replyTo = replyTo, cancelReply = { replyId = null })
        }
    }) {
        Transcript(bot, chatState, store, connect, PaddingValues(start = 16.dp, end = 16.dp, top = top + 60.dp,
            bottom = with(density) { composerHeight.toDp() } + 8.dp), indicatorTop = top + 50.dp, openRoutine = openRoutine, reply = reply,
            addFallback = { details = DetailsRequest(botId = it.id) }, openArtifact = { viewing = it }, openCall = { callLine = it }) { peer = it }
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
@Composable private fun ThreadHeader(bot: Bot, state: AppState, store: AppStore, openDetails: () -> Unit) {
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
                ConnectionSubtitle(state)
            }
        }
        // The corner is the computer: it pulses while this bot is operating it (watch, then take over).
        val screen = state.screen
        if (screen != null) {
            val operating = screen.agentBot == bot.id
            ComputerButton(operating, Modifier.align(Alignment.CenterEnd)) { store.openScreen(bot.id.takeIf { operating }) }
        }
    }
}

@Composable private fun ComputerButton(operating: Boolean, modifier: Modifier, action: () -> Unit) {
    val (source, pressed) = rememberPress()
    val pulse by rememberInfiniteTransition(label = "Operating").animateFloat(1f, 0.35f,
        infiniteRepeatable(tween(900), RepeatMode.Reverse), label = "Pulse")
    Box(modifier.size(44.dp).glass(Capsule(), pressed = pressed)
        .clickable(source, null, role = Role.Button, onClick = action).semantics { contentDescription = "Computer" },
        contentAlignment = Alignment.Center) {
        Symbol("desktopcomputer", Modifier.graphicsLayer { alpha = if (operating) pulse else 1f }, size = 22.dp, tint = Palette.text)
    }
}

/** The link to the computer: route icon (or a spinner) and a word. */
@Composable private fun ConnectionSubtitle(state: AppState) {
    Row(Modifier.clearAndSetSemantics { contentDescription = "${state.hostName}, ${connectionDescription(state)}" }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Box(Modifier.width(12.dp), contentAlignment = Alignment.Center) {
            when (val link = state.connection) {
                LinkState.Connecting -> Spinner(8.dp)
                is LinkState.Ready -> Symbol(if (link.route == HostRoute.Relay) "cloud" else "wifi", size = 11.dp, tint = Palette.secondary)
                else -> Symbol("wifi.slash", size = 11.dp, tint = Palette.secondary)
            }
        }
        Text(connectionLabel(state), style = Type.system(10f).medium, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** The messages: all this phone has, earlier ones from the computer near the top, following the newest. */
@Composable private fun Transcript(bot: Bot, state: AppState, store: AppStore, connect: ConnectionRequestActions, padding: PaddingValues,
    indicatorTop: Dp, openRoutine: (String) -> Unit, reply: (Entry) -> Unit, addFallback: (Bot) -> Unit, openArtifact: (OpenArtifact) -> Unit,
    openCall: (String) -> Unit, openPeer: (String) -> Unit) {
    // What voice calls carried shows in each call's conversation, not in the chat.
    val entries = state.entries.filter { it.botId == bot.id && it.isChat }.hidingCalls()
    // A call's message that failed to send stays here, to be sent again.
    val pending = state.pending.filter { it.botId == bot.id && (it.call == null || it.status == SendStatus.Failed) }
    val items = chatItems(entries, pending)
    val working = bot.isWorking(bot.id) && state.connection is LinkState.Ready
    val more = bot.id !in state.historyComplete
    val details by ThinkingDetails.enabled.collectAsStateWithLifecycle()
    // Pull to refresh reconnects, like Reconnect in the roster's computer menu, until the link is back.
    ConversationList(items.lastOrNull()?.id, entries.lastOrNull()?.rev, working, padding, store::refreshConnection, indicatorTop) {
        if (more) item("earlier") {
            Box(Modifier.fillMaxWidth().padding(vertical = 14.dp).clearAndSetSemantics { contentDescription = "Loading earlier messages" },
                contentAlignment = Alignment.Center) { Spinner(14.dp) }
            // A chat shorter than the screen never scrolls, so showing the row is the trigger.
            LaunchedEffect(Unit) { if ("history" !in state.busy) store.loadHistory() }
        }
        if (items.isEmpty() && !more) item("intro") { IntroCard(bot, state) }
        items(items, key = { it.id }) { item ->
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
        if (working) item("working") { WorkingIndicator(bot, workingSteps(state.entries.filter { it.botId == bot.id }), details) }
    }
}
