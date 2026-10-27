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
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
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
 * artifact's draft, apart from the chat's.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable fun MessageComposer(bot: Bot, state: AppState, store: AppStore, onCall: (() -> Unit)?,
    onInterrupt: (() -> Unit)? = null, replyTo: ReplyQuote? = null, cancelReply: () -> Unit = {}, about: String? = null,
    draftKey: String = bot.id) {
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
    val clipboard = LocalClipboard.current
    val clips = LocalContext.current.getSystemService(ClipboardManager::class.java)
    val scope = rememberCoroutineScope()
    val photos = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(10)) { uris ->
        if (uris.isNotEmpty()) store.importFiles(uris) { store.addDraftFiles(draftKey, it) }
    }
    val documents = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isNotEmpty()) store.importFiles(uris) { store.addDraftFiles(draftKey, it) }
    }
    val receiver = remember(draftKey, importing, queuing) { ReceiveContentListener { content ->
        if (importing || queuing) content else {
            val uris = mutableListOf<android.net.Uri>()
            val remaining = content.consume { item -> item.uri?.takeIf { it.scheme == "content" }?.let { uris.add(it); true } ?: false }
            if (uris.isNotEmpty()) store.importFiles(uris) { store.addDraftFiles(draftKey, it) }
            remaining
        }
    } }
    val online = state.actualConnection is LinkState.Ready
    val empty = text.text.isBlank() && files.isEmpty()
    // Offline computers still take text when the relay can hold it; files need the computer.
    val canSend = !empty && !importing && !queuing && (online || state.actualConnection is LinkState.ComputerOffline && files.isEmpty())
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
    val mentions = if (bot.kind != "group") emptyList() else mentionQuery(typed)?.let { query ->
        members.filter { query.isEmpty() || it.name.contains(query, true) }
    }.orEmpty()
    fun clear() = text.edit { replace(0, length, "") }
    fun run(command: SlashCommand) {
        val skill = command.skill
        when {
            skill != null -> text.setTextAndPlaceCursorAtEnd("/$skill ")
            else -> { store.toggleFastMode(bot); clear() }
        }
    }
    fun send() {
        val command = if (files.isEmpty()) SlashCommands.builtIn(bot, typed, members) else null
        if (command != null) run(command) else store.sendDraft(draftKey, bot.id, typed, replyTo) { clear(); cancelReply() }
    }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
    AnimatedVisibility(commands.isNotEmpty(), enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
        exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
        CommandList(commands, ::run, Modifier.padding(horizontal = 16.dp))
    }
    AnimatedVisibility(mentions.isNotEmpty(), enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
        MentionChips(mentions, state.bots) { member -> text.setTextAndPlaceCursorAtEnd(typed.substring(0, typed.lastIndexOf('@')) + "@${member.name} ") }
    }
    Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 6.dp, bottom = 16.dp), verticalAlignment = Alignment.Bottom,
        horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        DropdownMenu({ buildList {
            add(MenuItem("Photos", icon = "photo.on.rectangle") { photos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) })
            add(MenuItem("Files", icon = "folder") { documents.launch(arrayOf("*/*")) })
            // Only the clip's description: reading the clip itself would show Android's paste notice.
            if (clipOffersFiles(clips.primaryClipDescription?.mimeTypes().orEmpty())) add(MenuItem("Paste", icon = "doc.on.clipboard") { scope.launch {
                val data = clipboard.getClipEntry()?.clipData
                val uris = if (data == null) emptyList() else (0 until data.itemCount).mapNotNull { data.getItemAt(it).uri?.takeIf { uri -> uri.scheme == "content" } }
                if (uris.isNotEmpty()) store.importFiles(uris) { store.addDraftFiles(draftKey, it) }
            } })
        } }, Modifier.size(FIELD_HEIGHT).glass(Capsule()), enabled = !queuing, label = "Add files") {
            Symbol("plus", size = 26.dp, tint = Palette.text)
        }
        Column(Modifier.weight(1f).glass(RoundedRectangle(24.dp)).animateContentSize(Motion.layoutSize)) {
            // Each quote keeps its own bar through the transition, so a cancelled one fades out with its text.
            AnimatedContent(replyTo?.text?.takeIf { about == null }, transitionSpec = { fadeIn(Motion.fade()) togetherWith fadeOut(Motion.fade()) },
                label = "Reply") { quote ->
                if (quote != null) Row(Modifier.padding(start = 18.dp, end = 6.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    QuoteLine(quote, Modifier.weight(1f))
                    IconButton("Cancel reply", "xmark", action = cancelReply)
                }
            }
            if (files.isNotEmpty()) Row(Modifier.horizontalScroll(rememberScrollState()).padding(start = 10.dp, end = 10.dp, top = 8.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                for (file in files) ComposerAttachment(file) { store.removeDraftFile(draftKey, file) }
            }
            Row(Modifier.padding(start = 18.dp, end = 6.dp, top = 6.dp, bottom = 6.dp), verticalAlignment = Alignment.Bottom,
                horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                // A hardware keyboard's Enter is a new line; Ctrl/Meta+Enter (⌘↩ on iOS) sends, and never stops or calls.
                val sendKey = Modifier.onPreviewKeyEvent { e ->
                    val ctrlEnter = e.type == KeyEventType.KeyDown && e.key == Key.Enter && (e.isCtrlPressed || e.isMetaPressed)
                    if (ctrlEnter && trailing == Trailing.Send && canSend) send()
                    ctrlEnter
                }
                BasicTextField(state = text, enabled = !queuing, modifier = Modifier.weight(1f).focusRequester(focus).contentReceiver(receiver).then(sendKey).padding(vertical = 10.dp),
                    textStyle = Type.body.copy(color = Palette.text), cursorBrush = SolidColor(Palette.accent),
                    lineLimits = TextFieldLineLimits.MultiLine(maxHeightInLines = 8),
                    decorator = { input -> Box { if (text.text.isEmpty()) Text(placeholder, style = Type.body, color = Palette.tertiary, maxLines = 1, overflow = TextOverflow.Ellipsis); input() } })
                TrailingButton(trailing, canSend, Modifier.padding(bottom = 4.dp)) {
                    when (trailing) {
                        Trailing.Interrupt -> onInterrupt?.invoke()
                        Trailing.Stop -> store.command("stop", body("botId" to bot.id))
                        Trailing.Call -> onCall?.invoke()
                        Trailing.Send -> send()
                    }
                }
                if (bot.kind != "group") ContextUsageIndicator(bot.contextUsage, COMPOSER_BUTTON_SIZE)
            }
        }
    }
    }
}

private val FIELD_HEIGHT = 54.dp
private val COMPOSER_BUTTON_SIZE = 34.dp

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
    val width by animateDpAsState(if (state == Trailing.Call) 46.dp else COMPOSER_BUTTON_SIZE, Motion.morph(), label = "Width")
    val (source, pressed) = rememberPress()
    Box(modifier.pressScale(pressed).size(width, COMPOSER_BUTTON_SIZE).clip(Capsule()).background(fill)
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

fun formatBytes(size: Long): String = when {
    size >= 1024 * 1024 -> "%.1f MB".format(size / (1024.0 * 1024))
    size >= 1024 -> "%.0f KB".format(size / 1024.0)
    else -> "$size bytes"
}
