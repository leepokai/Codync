package com.codync.android

import android.content.ClipDescription
import android.content.ClipboardManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.text.input.setTextAndPlaceCursorAtEnd
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.content.ReceiveContentListener
import androidx.compose.foundation.content.consume
import androidx.compose.foundation.content.contentReceiver
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.clearText
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isMetaPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.launch

/** What the composer's one trailing button does now; it morphs between them. */
private enum class Trailing { Interrupt, Stop, Call, Send }

/**
 * The message box under a chat, as iOS `Composer`: a glass + for files, a glass field whose
 * trailing button morphs between call, send, stop and interrupt. `onInterrupt` cuts the bot off
 * while it speaks in a voice call. `replyTo` is the message the next send quotes, shown on top. `about`, in an artifact's
 * viewer: the artifact's title, which the box names in place of the quote (its card) it sends with; `draftKey` is then the
 * artifact's draft, apart from the chat's. `queued`: the user's messages waiting behind the bot's work, shown on top ([QueuedMessages]).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable fun MessageComposer(bot: Bot, state: AppState, store: AppStore, onCall: (() -> Unit)?,
    onInterrupt: (() -> Unit)? = null, replyTo: ReplyQuote? = null, cancelReply: () -> Unit = {}, about: String? = null,
    draftKey: String = bot.id, queued: List<Entry> = emptyList()) {
    val draft = state.drafts[draftKey]
    val text = rememberTextFieldState(initialText = draft?.text.orEmpty())
    val files = draft?.files.orEmpty()
    val queuing = "draft-send:$draftKey" in state.busy
    val importing = "attachments" in state.busy
    // What this box sent to the store, so a draft set elsewhere (a routine request) can be told from our own echo.
    val echoes = remember(draftKey) { ArrayDeque<String>() }
    LaunchedEffect(text, draftKey) {
        snapshotFlow { text.text.toString() }.collect {
            echoes.addLast(it); while (echoes.size > 32) echoes.removeFirst()
            store.editDraft(draftKey, it)
        }
    }
    // Choosing a message to reply to goes straight to typing.
    val focus = remember { FocusRequester() }
    LaunchedEffect(replyTo?.id) { if (replyTo != null && about == null) focus.requestFocus() }
    LaunchedEffect(draft?.text) {
        val external = draft?.text.orEmpty()
        if (external != text.text.toString() && external !in echoes) text.setTextAndPlaceCursorAtEnd(external)
    }
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // Dictation stops with its chat or draft (another chat, an artifact's box) and when the box goes.
    val dictation = remember(draftKey) { Dictation(context, scope) }
    DisposableEffect(dictation) { onDispose { dictation.stop() } }
    val receiver = fileReceiver(store, draftKey, importing || queuing)
    val online = state.actualConnection is LinkState.Ready
    val empty = text.text.isBlank() && files.isEmpty()
    // Offline computers still take text (`holdsText`). Files need the computer.
    val canSend = !empty && !importing && !queuing && (online || state.holdsText && files.isEmpty())
    val working = bot.isWorking(bot.id)
    val bare = text.text.isEmpty() && files.isEmpty()
    val trailing = when {
        onInterrupt != null && bare -> Trailing.Interrupt
        working && bare -> Trailing.Stop
        onCall != null && bare -> Trailing.Call
        else -> Trailing.Send
    }
    val placeholder = when {
        about != null -> tr("Message about “{title}”").replace("{title}", about)
        replyTo != null -> tr("Reply…")
        bot.kind == "group" -> tr("Ask everyone · @ to ask one bot")
        working -> tr("Queue a message for {name}").replace("{name}", bot.name)
        else -> tr("Ask {name}").replace("{name}", bot.name)
    }
    val typed = text.text.toString()
    val members = bot.memberBots(state.bots)
    val commands = SlashCommands.query(typed)?.let { SlashCommands.list(bot, it, state.plugins?.skills.orEmpty(), members) }.orEmpty()
    // Starting a command: list the computer's skills again, once each time the draft becomes "/".
    val startingCommand = typed == "/"
    LaunchedEffect(startingCommand) { if (startingCommand && online) store.refreshPlugins() }
    // The one way a group's box differs: @ asks one of its bots.
    val mentions = if (bot.kind != "group") emptyList() else mentionQuery(typed, members)?.let { query ->
        members.filter { query.isEmpty() || it.name.contains(query, true) }
    }.orEmpty()
    fun clear() = text.edit { replace(0, length, "") }
    fun mention(member: Bot) = text.setTextAndPlaceCursorAtEnd(typed.substring(0, typed.lastIndexOf('@')) + "@${member.name} ")
    fun run(command: SlashCommand) {
        if (command.fills) text.setTextAndPlaceCursorAtEnd("/${command.name} ") else { store.toggleFastMode(bot); clear() }
    }
    fun sendNow() {
        // A start still waiting (the key check) never opens the microphone after the send.
        dictation.stop()
        // The box now, not as composed: a send after dictation has the transcript too.
        val message = text.text.toString()
        val command = if (files.isEmpty()) SlashCommands.builtIn(bot, message, members) else null
        if (command != null) run(command) else store.sendDraft(draftKey, bot.id, message, replyTo) { clear(); cancelReply() }
    }
    // The composition current when the transcript arrives: a reply cancelled while it transcribes stays cancelled.
    val latestSend by rememberUpdatedState(::sendNow)
    // Sent once the words still on their way are in the box, with what was typed.
    fun send() = if (dictation.busy) dictation.finish { latestSend() } else sendNow()
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
    QueuedMessages(queued, state, store, Modifier.padding(horizontal = 16.dp))
    AnimatedVisibility(commands.isNotEmpty(), enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
        exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
        CommandList(commands, { agentRegistry(state, it) }, { backendName(state, it) }, ::run, Modifier.padding(horizontal = 16.dp))
    }
    AnimatedVisibility(mentions.isNotEmpty(), enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
        exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
        MentionList(mentions, state.bots, ::mention, Modifier.padding(horizontal = 16.dp))
    }
    Column(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 6.dp, bottom = 16.dp).glass(RoundedRectangle(24.dp))
        .animateContentSize(Motion.layoutSize)) {
        // Each quote keeps its own bar through the transition, so a cancelled one fades out with its text.
        AnimatedContent(replyTo?.text?.takeIf { about == null }, transitionSpec = { fadeIn(Motion.fade()) togetherWith fadeOut(Motion.fade()) },
            label = "Reply") { quote ->
            if (quote != null) Row(Modifier.padding(start = 18.dp, end = 6.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                QuoteLine(quote, Modifier.weight(1f))
                IconButton("Cancel reply", "xmark", action = cancelReply)
            }
        }
        DraftFiles(files, store, draftKey)
        ComposerInput(text, placeholder, trailing, canSend, enabled = !queuing, field = Modifier.focusRequester(focus).contentReceiver(receiver),
            leading = { AddFilesButton(store, draftKey, enabled = !queuing) }, accessory = {
            // A bot's own chat: a group's bots each run on their own.
            if (bot.kind != "group") Box(Modifier.weight(1f, fill = false)) { ModelChip(bot, state, store) }
            DictationButton(dictation, state, store, 34.dp, { text.text.toString() }) { change ->
                text.setTextAndPlaceCursorAtEnd(change(text.text.toString()))
            }
        }) {
            when (trailing) {
                Trailing.Interrupt -> onInterrupt?.invoke()
                Trailing.Stop -> store.command("stop", body("botId" to bot.id))
                // The call takes the microphone.
                Trailing.Call -> { dictation.stop(); onCall?.invoke() }
                Trailing.Send -> send()
            }
        }
    }
    }
}

/**
 * The chat's box under a task's thread, with its + for files where the computer takes them ([takesFiles]) but no
 * microphone: [send] takes what was typed (the draft's files go with it), only while the computer is there to take it.
 * What is typed and added is saved under [draftKey], as a chat's draft is. The messages waiting behind the thread's turn
 * show on top as a chat's do ([queued], with [steer] and [remove]). [accessory]: what the thread runs on, in the box's row;
 * while the thread is [working], an empty box's button is Stop ([stop]). Send waits while the box isn't [ready]; [send]
 * empties the box when it calls its second argument.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable internal fun ThreadComposer(state: AppState, store: AppStore, draftKey: String, takesFiles: Boolean, queued: List<Entry> = emptyList(),
    steer: (Entry) -> Unit = {}, remove: (Entry) -> Unit = {}, working: Boolean = false, stop: () -> Unit = {}, accessory: @Composable RowScope.() -> Unit,
    placeholder: String = tr("Message this thread"), ready: Boolean = true, send: (String, clear: () -> Unit) -> Unit) {
    val text = rememberTextFieldState(initialText = state.drafts[draftKey]?.text.orEmpty())
    LaunchedEffect(text, draftKey) { snapshotFlow { text.text.toString() }.collect { store.editDraft(draftKey, it) } }
    val files = state.drafts[draftKey]?.files.orEmpty()
    val importing = "attachments" in state.busy
    val canSend = (text.text.isNotBlank() || files.isNotEmpty()) && !importing && ready && state.actualConnection is LinkState.Ready
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        QueuedMessages(queued, state, store, Modifier.padding(horizontal = 16.dp), steer, remove)
        Column(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 6.dp, bottom = 16.dp).glass(RoundedRectangle(24.dp))
            .animateContentSize(Motion.layoutSize)) {
            DraftFiles(files, store, draftKey)
            val trailing = if (working && text.text.isEmpty() && files.isEmpty()) Trailing.Stop else Trailing.Send
            ComposerInput(text, placeholder, trailing, canSend,
                field = Modifier.semantics { contentDescription = placeholder }.contentReceiver(fileReceiver(store, draftKey, importing || !takesFiles)),
                leading = { if (takesFiles) AddFilesButton(store, draftKey, enabled = true) }, accessory = accessory) {
                if (trailing == Trailing.Stop) stop()
                else send(text.text.toString()) { text.clearText() }
            }
        }
    }
}

/** What is dropped or pasted into a box's field: content files go to draft [draftKey]; nothing is taken while [busy]. */
@OptIn(ExperimentalFoundationApi::class)
@Composable private fun fileReceiver(store: AppStore, draftKey: String, busy: Boolean) = remember(draftKey, busy) { ReceiveContentListener { content ->
    if (busy) content else {
        val uris = mutableListOf<android.net.Uri>()
        val remaining = content.consume { item -> item.uri?.takeIf { it.scheme == "content" }?.let { uris.add(it); true } ?: false }
        if (uris.isNotEmpty()) store.importFiles(uris) { store.addDraftFiles(draftKey, it) }
        remaining
    }
} }

/** A box's +, a light circle as the mic's capsule: photos, files, or the files the clipboard holds, added to draft [draftKey]. */
@Composable private fun AddFilesButton(store: AppStore, draftKey: String, enabled: Boolean) {
    val clipboard = LocalClipboard.current
    val clips = LocalContext.current.getSystemService(ClipboardManager::class.java)
    val scope = rememberCoroutineScope()
    val photos = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(10)) { uris ->
        if (uris.isNotEmpty()) store.importFiles(uris) { store.addDraftFiles(draftKey, it) }
    }
    val documents = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isNotEmpty()) store.importFiles(uris) { store.addDraftFiles(draftKey, it) }
    }
    DropdownMenu({ buildList {
        add(MenuItem("Photos", icon = "photo.on.rectangle") { photos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) })
        add(MenuItem("Files", icon = "folder") { documents.launch(arrayOf("*/*")) })
        // Only the clip's description: reading the clip itself would show Android's paste notice.
        if (clipOffersFiles(clips.primaryClipDescription?.mimeTypes().orEmpty())) add(MenuItem("Paste", icon = "doc.on.clipboard") { scope.launch {
            val data = clipboard.getClipEntry()?.clipData
            val uris = if (data == null) emptyList() else (0 until data.itemCount).mapNotNull { data.getItemAt(it).uri?.takeIf { uri -> uri.scheme == "content" } }
            if (uris.isNotEmpty()) store.importFiles(uris) { store.addDraftFiles(draftKey, it) }
        } })
    } }, Modifier.size(34.dp).clip(Capsule()).background(Palette.bubbleAgent), enabled = enabled, label = "Add files") {
        Symbol("plus", size = 20.dp, tint = Palette.text)
    }
}

/** The files of draft [draftKey] above its field, each with its remove button. */
@Composable private fun DraftFiles(files: List<OutgoingFile>, store: AppStore, draftKey: String) {
    if (files.isNotEmpty()) Row(Modifier.horizontalScroll(rememberScrollState()).padding(start = 10.dp, end = 10.dp, top = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        for (file in files) ComposerAttachment(file) { store.removeDraftFile(draftKey, file) }
    }
}

/**
 * A composer's two layers, as on the desktop: its text on top, and under it [leading] (the +), [accessory] (what the bot
 * runs on, the microphone) and the one [trailing] button, which runs [act]. [field]: what the text takes besides typing
 * (focus, pasted files).
 */
@Composable private fun ComposerInput(text: TextFieldState, placeholder: String, trailing: Trailing, canSend: Boolean, enabled: Boolean = true,
    field: Modifier = Modifier, leading: @Composable () -> Unit = {}, accessory: @Composable RowScope.() -> Unit = {}, act: () -> Unit) {
    Column(Modifier.padding(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        // A hardware keyboard's Enter is a new line; Ctrl/Meta+Enter (⌘↩ on iOS) sends, and never stops or calls.
        val sendKey = Modifier.onPreviewKeyEvent { e ->
            val ctrlEnter = e.type == KeyEventType.KeyDown && e.key == Key.Enter && (e.isCtrlPressed || e.isMetaPressed)
            if (ctrlEnter && trailing == Trailing.Send && canSend) act()
            ctrlEnter
        }
        BasicTextField(state = text, enabled = enabled, modifier = Modifier.fillMaxWidth().then(field).then(sendKey).padding(start = 10.dp, end = 10.dp, top = 8.dp),
            textStyle = Type.body.copy(color = Palette.text), cursorBrush = SolidColor(Palette.accent),
            lineLimits = TextFieldLineLimits.MultiLine(maxHeightInLines = 8),
            decorator = { input -> Box { if (text.text.isEmpty()) Text(placeholder, style = Type.body, color = Palette.tertiary, maxLines = 1, overflow = TextOverflow.Ellipsis); input() } })
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            leading()
            Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End), content = accessory)
            TrailingButton(trailing, canSend, Modifier, act)
        }
    }
}

/** One button that morphs between call, send, stop and interrupt: its symbol swaps, its fill crossfades and the capsule narrows. */
@Composable private fun TrailingButton(state: Trailing, canSend: Boolean, modifier: Modifier, action: () -> Unit) {
    val enabled = state != Trailing.Send || canSend
    val fill by animateColorAsState(when {
        state == Trailing.Interrupt -> Palette.danger
        state == Trailing.Send && !canSend -> Palette.accentDim
        else -> Palette.accentFill
    }, Motion.morph(), label = "Fill")
    val ink by animateColorAsState(when {
        state == Trailing.Interrupt -> Color.White
        state == Trailing.Send && !canSend -> Palette.tertiary
        else -> Palette.onAccent
    }, Motion.morph(), label = "Ink")
    val width by animateDpAsState(if (state == Trailing.Call) 46.dp else 34.dp, Motion.morph(), label = "Width")
    val (source, pressed) = rememberPress()
    Box(modifier.pressScale(pressed).size(width, 34.dp).clip(Capsule()).background(fill)
        .clickable(source, null, enabled = enabled, role = Role.Button, onClick = action)
        .semantics { contentDescription = when (state) { Trailing.Interrupt -> "Interrupt"; Trailing.Stop -> "Stop"; Trailing.Call -> "Call"; Trailing.Send -> "Send" } },
        contentAlignment = Alignment.Center) {
        AnimatedContent(state, transitionSpec = { (fadeIn(Motion.morph()) + scaleIn(Motion.morph(), 0.6f)) togetherWith
            (fadeOut(Motion.morph()) + scaleOut(Motion.morph(), 0.6f)) }, label = "Symbol") { shown ->
            when (shown) {
                Trailing.Interrupt, Trailing.Stop -> Symbol("stop.fill", size = 16.dp, tint = ink)
                Trailing.Call -> Symbol("waveform", size = 20.dp, tint = ink)
                Trailing.Send -> Symbol("arrow.up", size = 19.dp, tint = ink)
            }
        }
    }
}

/**
 * Whether the clipboard holds what Paste can attach (iOS: images or URLs), from its MIME types alone.
 * Text, HTML and intents carry no file; any other type comes from a URI item.
 */
internal fun clipOffersFiles(mimeTypes: List<String>): Boolean =
    mimeTypes.any { it !in listOf(ClipDescription.MIMETYPE_TEXT_PLAIN, ClipDescription.MIMETYPE_TEXT_HTML, ClipDescription.MIMETYPE_TEXT_INTENT) }

private fun ClipDescription.mimeTypes(): List<String> = (0 until mimeTypeCount).map(::getMimeType)

