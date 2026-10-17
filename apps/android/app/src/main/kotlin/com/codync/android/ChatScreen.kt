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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
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
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * One endless conversation with a bot or a group, as iOS `ThreadView`: only deliberate messages
 * show; tool calls and thinking live in "Full conversation". A thread opens over it as a sheet.
 */
@Composable fun ChatScreen(state: AppState, store: AppStore, edit: (Bot) -> Unit) {
    val bot = state.bots.firstOrNull { it.id == state.selectedBot } ?: return
    key(state.contextId, state.generation, bot.id) { ThreadView(bot, state, store, edit) }
}

@Composable private fun ThreadView(bot: Bot, state: AppState, store: AppStore, edit: (Bot) -> Unit) {
    val context = LocalContext.current
    var confirmNewSession by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    var routines by remember { mutableStateOf(false) }
    // The bot whose conversation with this one is open.
    var peer by remember(bot.id) { mutableStateOf<String?>(null) }
    var micDenied by remember { mutableStateOf(false) }
    var setupEntry by rememberSaveable { mutableStateOf<String?>(null) }
    var app by remember { mutableStateOf<ComposioApp?>(null) }
    var connector by remember { mutableStateOf<MarketConnector?>(null) }
    val connect = ConnectionRequestActions({ id, item -> setupEntry = id; app = item }, { id, item -> setupEntry = id; connector = item })
    val startCall = rememberStartVoice(bot.id, state, store) { micDenied = true }
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    var composerHeight by remember { mutableIntStateOf(0) }
    val density = LocalDensity.current
    // The main chat ignores an open thread: it stays put underneath.
    val chatState = state.copy(thread = null, trace = false)
    GlassStage(Modifier.fillMaxSize().background(Palette.background), chrome = {
        Box(Modifier.fillMaxWidth().height(top + 64.dp).background(Brush.verticalGradient(
            0f to Palette.background, 0.55f to Palette.background.copy(alpha = 0.85f), 1f to Color.Transparent)))
        ThreadHeader(bot, state, store, edit, { store.showTrace(true) }, { routines = true }, { confirmNewSession = true }, { confirmDelete = true })
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
            if (mismatch != null) UpdateNeededCard(state, mismatch, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp))
            else MessageComposer(bot, null, chatState, store, onCall = if (state.voice == null && state.actualConnection is LinkState.Ready) startCall else null)
        }
    }) {
        Transcript(bot, chatState, store, connect, PaddingValues(start = 16.dp, end = 16.dp, top = top + 60.dp,
            bottom = with(density) { composerHeight.toDp() } + 8.dp)) { peer = it }
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
    CodyncSheet(routines, { routines = false }) { RoutinesScreen(bot, state, store) }
    CodyncSheet(peer, { peer = null }) { BotConversationView(bot, it, state, store) }
    CodyncSheet(app, { app = null; setupEntry = null }) { ComposioConnectSheet(it, state, store, setupEntry) { store.refreshPlugins() } }
    CodyncSheet(connector, { connector = null; setupEntry = null }) { InstallConnectorSheet(it, state, store, setupEntry) }
}

/** The glass back button, the title (its menu holds the bot's actions) and the connection under it. */
@Composable private fun ThreadHeader(bot: Bot, state: AppState, store: AppStore, edit: (Bot) -> Unit, trace: () -> Unit,
    routines: () -> Unit, newSession: () -> Unit, delete: () -> Unit) {
    Box(Modifier.fillMaxWidth().statusBarsPadding().padding(top = 6.dp).padding(horizontal = 16.dp).height(44.dp)) {
        val (source, pressed) = rememberPress()
        Box(Modifier.align(Alignment.CenterStart).size(44.dp).glass(Capsule(), pressed = pressed)
            .then(Modifier.plainButton { store.openBot(null) }).semantics { contentDescription = "Back" }, contentAlignment = Alignment.Center) {
            Symbol("chevron.left", size = 32.dp, tint = Palette.text)
        }
        val host = state.computers.firstOrNull()?.name.orEmpty()
        DropdownMenu({ buildList {
            add(MenuItem("$host · ${connectionLabel(state)}", enabled = false))
            add(MenuItem("Reconnect", icon = "arrow.clockwise") { store.connect() })
            add(MenuItem("Details", icon = "info.circle", divider = true) { edit(bot) })
            add(MenuItem("Full conversation", icon = "list.bullet.rectangle", action = trace))
            val pin = MenuItem(if (bot.pinned) "Unpin" else "Pin", icon = "pin") {
                store.command("updateBot", kotlinx.serialization.json.buildJsonObject {
                    put("id", kotlinx.serialization.json.JsonPrimitive(bot.id)); put("pinned", kotlinx.serialization.json.JsonPrimitive(!bot.pinned))
                }, "roster:" + bot.id)
            }
            if (bot.kind == "group") {
                add(MenuItem("Edit group", icon = "person.2") { edit(bot) }); add(pin)
                add(MenuItem("Delete group", icon = "trash", destructive = true, divider = true, action = delete))
            } else {
                add(MenuItem("Edit profile", icon = "pencil") { edit(bot) }); add(pin)
                add(MenuItem("Routines", icon = "clock.arrow.circlepath", action = routines))
                add(MenuItem("New session", icon = "arrow.counterclockwise", action = newSession))
                add(MenuItem("Delete bot", icon = "trash", destructive = true, divider = true, action = delete))
            }
        } }, Modifier.align(Alignment.Center), label = "${bot.name} actions") {
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
        Text(connectionLabel(state), style = Type.system(10f).medium, color = Palette.secondary, maxLines = 1)
    }
}

/** The messages: all this phone has, earlier ones from the computer near the top, following the newest. */
@Composable private fun Transcript(bot: Bot, state: AppState, store: AppStore, connect: ConnectionRequestActions, padding: PaddingValues,
    openPeer: (String) -> Unit) {
    val entries = state.entries.filter { it.botId == bot.id && it.threadId == null && it.isChat }
    val pending = state.pending.filter { it.botId == bot.id && it.threadId == null }
    val items = chatItems(entries, pending)
    val working = bot.status in listOf("working", "needsInput") && bot.workingThread == null && state.connection is LinkState.Ready
    val list = rememberLazyListState()
    val more = bot.id !in state.historyComplete
    Follow(list, items.lastOrNull()?.id, entries.lastOrNull()?.rev, working, padding.calculateBottomPadding())
    LazyColumn(Modifier.fillMaxSize(), state = list, contentPadding = padding) {
        if (more) item("earlier") {
            Box(Modifier.fillMaxWidth().padding(vertical = 14.dp), contentAlignment = Alignment.Center) { Spinner(14.dp) }
            // A chat shorter than the screen never scrolls, so showing the row is the trigger.
            LaunchedEffect(Unit) { if ("history" !in state.busy) store.loadHistory() }
        }
        if (items.isEmpty() && !more) item("intro") { IntroCard(bot, state) }
        items(items, key = { it.id }) { item ->
            when (item) {
                is ChatItem.Separator -> TimeSeparator(item.at)
                is ChatItem.Message -> ChatRow(item.entry, item.groupStart, bot, state, store, openTrace = { store.showTrace(true) },
                    reply = { store.openBot(bot.id, item.entry.id) }, connect = connect)
                is ChatItem.Pending -> PendingBubble(item.send, item.groupStart, state, store)
                is ChatItem.Exchanges -> BotMessageRow(item.group, state) { openPeer(item.group.peerId) }
            }
        }
        if (working) item("working") { WorkingIndicator(bot, currentThinking(state.entries, bot.id, null)) }
    }
}

/** Stays on the newest message until the reader scrolls away; scrolling back down follows again. */
@Composable private fun Follow(list: LazyListState, last: String?, rev: Long?, working: Boolean, bottom: Dp = 0.dp) {
    var following by remember { mutableStateOf(true) }
    LaunchedEffect(list) {
        // Only the reader's own scroll decides: new content arriving (even while Codync is in the background) keeps the follow.
        var touched = false
        launch { list.interactionSource.interactions.collect { if (it is DragInteraction.Start) touched = true } }
        snapshotFlow { list.isScrollInProgress }.collect { scrolling ->
            if (!scrolling && touched) { touched = false; following = !list.canScrollForward }
        }
    }
    LaunchedEffect(Unit) {
        val count = snapshotFlow { list.layoutInfo.totalItemsCount }.first { it > 0 }
        list.scrollToItem(count - 1, Int.MAX_VALUE)
    }
    // A new message, or the box growing (keyboard, more lines): stay on the newest.
    LaunchedEffect(last, rev, working, bottom) {
        val count = list.layoutInfo.totalItemsCount
        if (!following || count == 0) return@LaunchedEffect
        list.animateScrollToItem(count - 1)
        list.scrollToItem(count - 1, Int.MAX_VALUE)
    }
}

/** A thread: the message it started on, its replies, and a box to continue there. */
@Composable private fun RepliesView(bot: Bot, rootId: String, state: AppState, store: AppStore, connect: ConnectionRequestActions) {
    val root = state.entries.firstOrNull { it.botId == bot.id && it.id == rootId }
    val replies = state.entries.filter { it.botId == bot.id && it.threadId == rootId && it.isChat }
    val pending = state.pending.filter { it.botId == bot.id && it.threadId == rootId }
    val working = bot.status in listOf("working", "needsInput") && bot.workingThread == rootId && state.connection is LinkState.Ready
    var composerHeight by remember { mutableIntStateOf(0) }
    val density = LocalDensity.current
    val dismiss = LocalDismissModal.current
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            Row(Modifier.fillMaxWidth().height(56.dp).padding(start = 20.dp, end = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Thread", style = Type.body.semibold, color = Palette.text)
                    Text(bot.name, style = Type.caption, color = Palette.tertiary, maxLines = 1)
                }
                IconButton("Close thread", "xmark", action = dismiss)
            }
            Box(Modifier.fillMaxWidth().height(0.5.dp).background(Palette.border))
            val list = rememberLazyListState()
            val items = chatItems(replies, pending)
            Follow(list, items.lastOrNull()?.id, replies.lastOrNull()?.rev, working)
            LazyColumn(Modifier.weight(1f), state = list, contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp,
                bottom = with(density) { composerHeight.toDp() } + 8.dp)) {
                if (root != null) {
                    item("root") { ChatRow(root, true, bot, state, store, openTrace = { store.showTrace(true) }, reply = null, connect = connect) }
                    item("count") {
                        val count = (root.data["thread"] as? kotlinx.serialization.json.JsonObject)?.get("count")
                            ?.let { (it as? kotlinx.serialization.json.JsonPrimitive)?.content?.toIntOrNull() } ?: 0
                        Row(Modifier.padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text(if (count == 0) "No replies yet" else if (count == 1) "1 reply" else "$count replies", style = Type.caption, color = Palette.tertiary)
                            Box(Modifier.weight(1f).height(1.dp).background(Palette.border))
                        }
                    }
                }
                items(items.filter { it !is ChatItem.Separator }, key = { it.id }) { item ->
                    when (item) {
                        is ChatItem.Message -> ChatRow(item.entry, item.groupStart, bot, state, store, openTrace = { store.showTrace(true) }, reply = null, connect = connect)
                        is ChatItem.Pending -> PendingBubble(item.send, item.groupStart, state, store)
                        is ChatItem.Separator, is ChatItem.Exchanges -> {}
                    }
                }
                if (working) item("working") { WorkingIndicator(bot, currentThinking(state.entries, bot.id, rootId)) }
            }
        }
        Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth().onSizeChanged { composerHeight = it.height }) {
            MessageComposer(bot, rootId, state, store, onCall = null)
        }
    }
}
