package com.codync.android

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.core.artifactDraftKey
import com.codync.android.core.artifactQuote
import com.codync.android.design.*

/**
 * The artifact a viewer shows, with what's known before its details arrive: a title, and the card to reply to. `botId`: the
 * bot that owns it, not always the chat's (a group member's Library, from the group).
 */
internal data class OpenArtifact(val botId: String, val id: String, val title: String, val entryId: String?)

/**
 * `content` over everything while `open` is set, sliding up like a full-screen cover; it keeps its artifact while it slides
 * away. The caller's `content` brings the state, so the layer follows it as a sheet's content does.
 */
@Composable internal fun ArtifactLayer(open: OpenArtifact?, content: @Composable (OpenArtifact) -> Unit) {
    val visible = remember { MutableTransitionState(false) }
    visible.targetState = open != null
    var last by remember { mutableStateOf(open) }
    if (open != null) last = open
    if (!visible.currentState && !visible.targetState) return
    Overlay {
        AnimatedVisibility(visible, enter = slideInVertically(Motion.layoutOffset) { it } + fadeIn(Motion.fade()),
            exit = slideOutVertically(Motion.layoutOffset) { it } + fadeOut(Motion.fade())) {
            last?.let { content(it) }
        }
    }
}

/**
 * An artifact full size (docs/features/artifacts.md): close, its title (tap to rename) and share on top, a message about it
 * at the bottom. It reads the artifact again when the bot publishes a newer version.
 */
@Composable internal fun ArtifactViewer(open: OpenArtifact, bot: Bot, state: AppState, store: AppStore, close: () -> Unit) {
    val artifacts = state.artifacts
    val artifact = artifacts.details[open.id]
    val title = artifact?.title ?: open.title
    val newest = artifacts.newest(open.id)
    val online = state.actualConnection is LinkState.Ready
    LaunchedEffect(open.id, newest, online) { store.loadArtifact(bot.id, open.id) }
    var renaming by remember { mutableStateOf(false) }
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Artifact))
    // The artifact crashed its renderer: this view is done; opening the artifact again starts a new one.
    var gone by remember(open.id) { mutableStateOf(false) }
    BackHandler(onBack = close)
    Column(Modifier.fillMaxSize().background(Palette.background).statusBarsPadding().navigationBarsPadding().imePadding()) {
        ScreenHeader({ IconButton("Close artifact", "xmark", action = close) }, {
            Text(title, Modifier.padding(horizontal = 56.dp).plainButton { renaming = true }, style = Type.body.semibold,
                color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }, { IconButton("Share artifact", "square.and.arrow.up") { store.shareArtifact(bot.id, open.id, title) } })
        Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
            val html = artifacts.html[open.id]?.text
            val failure = artifacts.failures[open.id]
            when {
                gone -> ArtifactFailure(null)
                // A newer version that couldn't be read leaves the one shown.
                html != null -> ArtifactWebView(open.id, html, interactive = true, gone = { gone = true }, Modifier.fillMaxSize())
                failure != null -> ArtifactFailure(failure)
                else -> Spinner(20.dp)
            }
        }
        // No card (a routine run published it): nothing a message could reply to.
        val quote = (artifact?.entryId ?: open.entryId)?.let { artifactQuote(it, bot.id, open.id, title) }
        if (quote != null) MessageComposer(bot, state, store, onCall = null, replyTo = quote, about = title, draftKey = artifactDraftKey(bot.id, open.id))
    }
    RenameArtifactDialog(title.takeIf { renaming }, { renaming = false }) { store.renameArtifact(bot.id, open.id, it) }
}

@Composable private fun ArtifactFailure(detail: String?) {
    Column(Modifier.padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(tr("Couldn't open the artifact"), style = Type.headline, color = Palette.text, textAlign = TextAlign.Center)
        if (detail != null) Text(detail, style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
    }
}

/** "Rename artifact" for the artifact titled `current`, shown while it's set. */
@Composable internal fun RenameArtifactDialog(current: String?, dismiss: () -> Unit, rename: (String) -> Unit) {
    var text by remember { mutableStateOf("") }
    // Set when it opens; kept while it fades away.
    LaunchedEffect(current) { if (current != null) text = current }
    CodyncDialog("Rename artifact", current != null, dismiss, field = {
        val focus = remember { FocusRequester() }
        LaunchedEffect(Unit) { focus.requestFocus() }
        PlainTextField(text, { text = it }, Modifier.fillMaxWidth().focusRequester(focus).pill(Palette.background), "Artifact title", singleLine = true)
    }) { listOf(DialogAction("Save") { text.trim().takeIf { it.isNotEmpty() && it != current }?.let(rename) }) }
}
