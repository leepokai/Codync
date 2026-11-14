package com.codync.android

import android.graphics.ImageDecoder
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.BotDestination
import com.codync.android.core.LinkState
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Something another app shared (`ShareActivity`; the iPhone's Share extension): its files, a message (the shared text,
 * editable), then the bot it goes to, Recent's first and the most recent picked. Send hands it to that chat like a message
 * from its box and opens the chat.
 */
@Composable internal fun ShareToBot(shared: SharedContent, app: AccountContext, states: List<AppState>, accounts: AccountStore) {
    val resolver = LocalContext.current.contentResolver
    val scope = rememberCoroutineScope()
    val files by produceState<List<IncomingFile>?>(null, shared) {
        value = withContext(Dispatchers.IO) { shared.uris.map { IncomingFile.describe(resolver, it) } }
    }
    var message by remember(shared) { mutableStateOf(fitMessage(shared.text)) }
    var chosen by remember(shared) { mutableStateOf<ShareRecipient?>(null) }
    var sending by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val (recent, sections) = shareChoices(states, app.shareRecipients)
    val all = recent + sections.flatMap { it.second }
    val picked = if (chosen == null) recent.firstOrNull() else all.firstOrNull { it.recipient == chosen }
    val sendable = files?.filter { !it.tooLarge }.orEmpty()
    val online = picked?.state?.actualConnection is LinkState.Ready
    // As the chat's box: offline computers still take text (`holdsText`); files need the computer.
    val waitsForComputer = picked != null && sendable.isNotEmpty() && !online
    val canSend = picked != null && files != null && !sending && (message.isNotBlank() || sendable.isNotEmpty()) &&
        (online || picked.state.holdsText && sendable.isEmpty())
    fun send() {
        val to = picked ?: return
        sending = true
        error = null
        scope.launch {
            // Done, it closes this sheet and opens the chat.
            try {
                accounts.sendShared(BotDestination(to.state.contextId, to.state.computer.id, to.bot.id), message, sendable.map { it.uri },
                    shared.pasted(message), to in recent)
            }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failed: Exception) { error = failed.message ?: SHARE_FAILED; sending = false }
        }
    }
    Column {
        ModalHeader("Send to a bot") {
            AnimatedContent(sending, transitionSpec = { fadeIn(Motion.fade()) togetherWith fadeOut(Motion.fade()) }, label = "Send") { busy ->
                if (busy) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner(18.dp) }
                else IconButton("Send", "arrow.up", enabled = canSend, action = ::send)
            }
        }
        CardForm {
            AnimatedVisibility(!files.isNullOrEmpty(), enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
                exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
                CardSection { for (file in files.orEmpty()) row { IncomingFileRow(file) } }
            }
            CardSection {
                row { PlainTextField(message, { message = it; error = null }, Modifier.fillMaxWidth(), "Add a message", maxLines = 6) }
            }
            val status = error ?: "Files go out once your computer is online.".takeIf { waitsForComputer }
            // The line keeps its text while it folds away.
            var shownStatus by remember { mutableStateOf("") }
            if (status != null) shownStatus = status
            AnimatedVisibility(status != null, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
                exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
                Text(tr(shownStatus), Modifier.padding(horizontal = 2.dp), style = Type.footnote, color = if (error != null) Palette.danger else Palette.secondary)
            }
            when {
                states.isEmpty() -> Text(tr("Open Codync and connect a computer first."), style = Type.body, color = Palette.secondary)
                all.isEmpty() -> Text(tr("No bots yet"), style = Type.body, color = Palette.secondary)
                else -> {
                    val several = states.size > 1
                    if (recent.isNotEmpty()) CardSection("Recent") {
                        for (choice in recent) row { ShareBotRow(choice, choice === picked, if (several) choice.state.hostName else null) { chosen = choice.recipient } }
                    }
                    for ((state, choices) in sections) if (choices.isNotEmpty()) CardSection(if (several) state.hostName else "Bots") {
                        for (choice in choices) row { ShareBotRow(choice, choice === picked, null) { chosen = choice.recipient } }
                    }
                }
            }
        }
    }
}

/** A bot to pick: avatar, name (and its computer's, in Recent), and a check once picked. */
@Composable private fun ShareBotRow(choice: ShareChoice, selected: Boolean, computer: String?, pick: () -> Unit) {
    Row(Modifier.fillMaxWidth().plainButton(role = Role.RadioButton, action = pick).semantics { this.selected = selected },
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        BotAvatar(choice.bot, choice.state.bots, 36.dp, animated = false)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(choice.bot.name, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (computer != null) Text(computer, style = Type.footnote, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        AnimatedVisibility(selected, enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
            Symbol("checkmark", size = 17.dp, tint = Palette.text)
        }
    }
}

/** A shared file: its picture (or its kind's symbol), name, and size, or that it's too large to go. */
@Composable private fun IncomingFileRow(file: IncomingFile) {
    val resolver = LocalContext.current.contentResolver
    val pixels = with(LocalDensity.current) { 40.dp.roundToPx() }
    val image by produceState<ImageBitmap?>(null, file.uri) {
        if (file.image) value = withContext(Dispatchers.IO) {
            runCatching { AttachmentFiles.thumbnail(ImageDecoder.createSource(resolver, file.uri), pixels).asImageBitmap() }.getOrNull()
        }
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.size(40.dp).clip(RoundedRectangle(8.dp)).background(Palette.bubbleAgent), contentAlignment = Alignment.Center) {
            val shown = image
            if (shown != null) Image(shown, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
            else Symbol(AttachmentIcon.symbol(file.name), size = 18.dp, tint = Palette.secondary)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(file.name, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            val size = file.size
            if (file.tooLarge) Text(tr("Larger than 100 MB"), style = Type.footnote, color = Palette.danger)
            else if (size != null) Text(formatBytes(size), style = Type.footnote, color = Palette.secondary)
        }
    }
}
