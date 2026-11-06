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
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
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
import androidx.compose.ui.draw.clip
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
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle

/**
 * One endless conversation with a bot or a group, as iOS `ThreadView`: only deliberate messages
 * show; tool calls and thinking live in "Full conversation". A thread opens over it as a sheet.
 */
@Composable fun ChatScreen(state: AppState, store: AppStore, edit: (Bot) -> Unit) {
    val bot = state.bots.firstOrNull { it.id == state.selectedBot } ?: return
    key(state.contextId, state.generation, bot.id) {
        BotAccentTheme(bot.avatarColor.takeIf { bot.kind != "group" }) {
            ThreadView(bot, state, store, edit)
        }
    }
}

@Composable private fun ThreadView(bot: Bot, state: AppState, store: AppStore, edit: (Bot) -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Chat))
    val context = LocalContext.current
    var confirmNewSession by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    var routines by remember { mutableStateOf(false) }
    var searching by remember { mutableStateOf(false) }
    // The routine the sheet opens on (a run's notice), or none for the list.
    var routineId by remember { mutableStateOf<String?>(null) }
    val openRoutine = { id: String? -> routineId = id; routines = true }
    var details by remember(bot.id) { mutableStateOf<DetailsRequest?>(null) }
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
    // The chat message the next one answers (`replyTo`).
    var replyingTo by remember(bot.id) { mutableStateOf<Entry?>(null) }
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    var composerHeight by remember { mutableIntStateOf(0) }
    val scroll = rememberConversationScroll()
    val density = LocalDensity.current
    // The main chat ignores an open thread: it stays put underneath.
    val chatState = state.copy(thread = null, trace = false)
    GlassStage(Modifier.fillMaxSize().background(Palette.background), chrome = {
        // Down to just above the header's bottom; the bottom one stops under Jump to latest.
        ScrollEdge(Edge.Top, top + 84.dp)
        ScrollEdge(Edge.Bottom, with(density) { composerHeight.toDp() } + 4.dp, Modifier.align(Alignment.BottomCenter))
        // 10 above the composer, as on iOS: the list already keeps 8 under its last row.
        JumpToLatest(scroll, Modifier.align(Alignment.BottomCenter).padding(bottom = with(density) { composerHeight.toDp() } + 10.dp))
        ThreadHeader(bot, state, store, { shown -> if (shown.kind == "group") edit(shown) else details = DetailsRequest() }, { store.showTrace(true) }, { openRoutine(null) }, { confirmNewSession = true }, { confirmDelete = true }, { searching = true })
        // The call floats over the chat under the header, as on iOS; the chat stays readable and usable.
        AnimatedVisibility(state.voice != null, Modifier.padding(top = top + 54.dp),
            enter = fadeIn(Motion.fade()) + slideInVertically(Motion.layoutOffset) { -it }, exit = fadeOut(Motion.fade()) + slideOutVertically(Motion.layoutOffset) { -it }) {
            state.voice?.let { CallBar(it, state, store) }
        }
        // Measured with the keyboard and navigation bar under it, so the last message clears them too.
        Column(Modifier.align(Alignment.BottomCenter).fillMaxWidth().onSizeChanged { composerHeight = it.height }
            .imePadding().navigationBarsPadding()) {
            val sharedPeers = remember(bot, state.bots) { sharedWorkspacePeers(bot, state.bots) }
            if (sharedPeers.isNotEmpty()) {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 4.dp).clip(Capsule()).background(Palette.surface).padding(horizontal = 12.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Symbol("exclamationmark.triangle", size = 12.dp, tint = Palette.warning)
                    Text(
                        "${sharedPeers.joinToString(", ") { it.name }} also active in ${bot.cwd.trimEnd('/', '\\').substringAfterLast('/').substringAfterLast('\\')}",
                        style = Type.caption,
                        color = Palette.warning,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            // Nothing sent now could be read on the other side.
            val mismatch = state.mismatch
            if (mismatch != null) UpdateNeededCard(state, mismatch, store::requestHostUpdate, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp))
            else if (viewing == null) MessageComposer(bot, null, chatState, store, onCall = if (state.voice == null && state.actualConnection is LinkState.Ready) startCall else null,
                replyingTo = replyingTo, cancelReply = { replyingTo = null },
                onInterrupt = if (state.voice?.botId == bot.id && state.voice.state.phase == VoicePhase.Speaking) store::interruptVoice else null)
        }
    }) {
        Transcript(bot, chatState, store, connect, scroll, PaddingValues(start = 16.dp, end = 16.dp, top = top + 60.dp,
            bottom = with(density) { composerHeight.toDp() } + 8.dp), indicatorTop = top + 50.dp, openRoutine = openRoutine, reply = { replyingTo = it }, openArtifact = { viewing = it }, openCall = { callLine = it }) { peer = it }
    }
    CodyncDialog("Start a new session?", confirmNewSession, { confirmNewSession = false },
        message = "The conversation stays here, but the agent starts with a fresh context.") {
        listOf(DialogAction("New session") { store.command("newSession", body("botId" to bot.id)) })
    }
    CodyncDialog("Delete ${bot.name}?", confirmDelete, { confirmDelete = false },
        message = if (bot.kind == "group") "Its bots and their own chats stay." else "Files it changed on your computer stay as they are.") {
        listOf(DialogAction(if (bot.kind == "group") "Delete group chat" else "Delete bot and its conversation", destructive = true) {
            store.deleteBot(bot) { store.openBot(null) }
        })
    }
    CodyncDialog("Microphone access is off", micDenied, { micDenied = false },
        message = "Allow the microphone in Android settings to talk to your bot.") {
        listOf(DialogAction("Open settings") { openAppSettings(context) })
    }
    // A thread over the chat, and "Full conversation" over either.
    CodyncSheet(state.thread, { store.openBot(bot.id) }) { root -> RepliesView(bot, root, state, store, connect) }
    CodyncSheet(state.trace, { store.showTrace(false) }) { TraceView(bot, state.thread, state) }
    CodyncSheet(searching, { searching = false }) { ConversationSearch(bot, store) { searching = false } }
    CodyncSheet(routines, { routines = false }, detents = setOf(SheetDetent.Medium, SheetDetent.Expanded), initialDetent = SheetDetent.Expanded) {
        RoutinesScreen(bot, state, store, routineId)
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

/** The messages: all this phone has, earlier ones from the computer near the top, following the newest. */
@Composable private fun Transcript(bot: Bot, state: AppState, store: AppStore, connect: ConnectionRequestActions, scroll: ConversationScroll, padding: PaddingValues,
    indicatorTop: Dp, openRoutine: (String) -> Unit, reply: (Entry) -> Unit, openArtifact: (OpenArtifact) -> Unit, openCall: (String) -> Unit, openPeer: (String) -> Unit) {
    val entries = state.entries.filter { it.botId == bot.id && it.threadId == null && it.isChat }.hidingCalls()
    val pending = state.pending.filter { it.botId == bot.id && it.threadId == null && (it.call == null || it.status == SendStatus.Failed) }
    val items = chatItems(entries, pending)
    val working = bot.isWorking(bot.id) && state.connection is LinkState.Ready
    val more = bot.id !in state.historyComplete
    val details by ThinkingDetails.enabled.collectAsStateWithLifecycle()
    // Pull to refresh reconnects, like Reconnect in the title menu, until the link is back.
    ConversationList(scroll, entries.map { it.id }, items.lastOrNull()?.id, entries.lastOrNull()?.rev, working, padding, store::refreshConnection, indicatorTop) {
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
                    if (routine != null) RoutineNotice(item.entry) { openRoutine(routine) }
                    else if (item.entry.kind == "notice" && item.entry.call != null) Box(Modifier.padding(top = 6.dp)) {
                        Column {
                            CallLineRow(item.entry) { openCall(item.entry.id) }
                            if (item.entry.data.text("source") == "voice_receipt") {
                                val seconds = item.entry.data["callSeconds"]?.jsonPrimitive?.longOrNull ?: 0
                                VoiceReceipt(item.entry, store, seconds, item.entry.data.text("voiceTranscriptText") ?: "No transcript was captured. Confirm only decisions you personally remember.")
                            }
                        }
                    }
                    else ChatRow(item.entry, item.groupStart, bot, state, store, openTrace = { store.showTrace(true) },
                        reply = { reply(item.entry) }, connect = connect, openArtifact = openArtifact)
                }
                is ChatItem.Pending -> PendingBubble(item.send, item.groupStart, state, store)
                is ChatItem.Exchanges -> BotMessageRow(item.group, state) { openPeer(item.group.peerId) }
            }
        }
        if (working) item("working") { WorkingIndicator(bot, workingSteps(state.entries.filter { it.botId == bot.id && it.threadId == null }), details) }
    }
}
