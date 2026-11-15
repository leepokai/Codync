package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.core.ScreenSize
import com.codync.android.core.ScreenViewport
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/**
 * The computer the bot lives on (iOS `BotComputerTab`): how it's connected, a view-only look at its screen once tapped (tap
 * the live picture to open the screen) and whether the bot may use it.
 */
@Composable internal fun BotComputerTab(bot: Bot, state: AppState, store: AppStore, uses: Boolean, online: Boolean, change: (Boolean) -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.BotComputer))
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        CardSection {
            row {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    ComputerHeader(state)
                    ScreenPreview(bot, state, store)
                }
            }
        }
        // Saved with the rest of the draft, so a save of the other settings never sends it back.
        state.screen?.let { screen ->
            CardSection {
                row {
                    SwitchRow("Use the computer", if (screen.controlReady) null else "On that computer, open Codync → Settings → Computer access to set it up first.",
                        uses, change, enabled = online)
                }
            }
        }
    }
}

/** The computer's badge (as on its heading and in Computer management), its name, and its connection as the computer filter says it. */
@Composable private fun ComputerHeader(state: AppState) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        ComputerBadge(state.hello?.text("device"), 20.dp, state.computer.color)
        Text(state.hostName, Modifier.weight(1f), style = Type.body.medium, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(tr(connectionLabel(state)), style = Type.footnote, color = if (state.unreachable) Palette.warning else Palette.secondary, maxLines = 1)
    }
}

/**
 * The computer's main display, live and view only, once tapped and while this tab shows with the app in front; it pauses
 * when nobody touches the app for a while. It lets go while the full viewer has the screen (one stream from this phone at a
 * time). Tapping the live picture opens that viewer: watching, when this bot is the one using the computer.
 */
@Composable private fun ScreenPreview(bot: Bot, state: AppState, store: AppStore) {
    val screen = state.screen
    val display = screen?.mainDisplay
    val inFront = LocalLifecycleOwner.current.lifecycle.currentStateAsState().value.isAtLeast(Lifecycle.State.STARTED)
    val available = state.connection is LinkState.Ready && screen?.available == true && display != null
    // Nothing streams until the picture is tapped; back from the background, it waits for a tap again.
    var prompt by remember { mutableStateOf<ScreenPrompt?>(ScreenPrompt.Start) }
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) { prompt = ScreenPrompt.Start }
    val watch = available && inFront && state.screenRequest == null && prompt == null
    var session by remember { mutableStateOf<ScreenSession?>(null) }
    DisposableEffect(watch, display?.id) {
        val next = display?.takeIf { watch }?.let { ScreenSession(store, it).also(ScreenSession::start) }
        session = next
        onDispose { next?.close(); session = null }
    }
    PauseWhenIdle(session, AnalyticsScreen.BotComputer) { prompt = ScreenPrompt.Paused }
    val live = session?.phase == ScreenSession.Phase.Live
    val agent = screen?.agentBot?.let { id -> state.bots.firstOrNull { it.id == id } }
    val label = tr("Open the screen")
    CompositionLocalProvider(LocalDark provides true) {
        BoxWithConstraints(Modifier.fillMaxWidth().aspectRatio(16f / 10f).clip(RoundedRectangle(12.dp)).background(Color.Black)
            .then(if (live) Modifier.plainButton { store.openScreen(bot.id.takeIf { screen?.agentBot == bot.id }) }
                .semantics { contentDescription = label } else Modifier)) {
            val s = session
            when {
                s != null && display != null -> {
                    val view = ScreenSize(maxWidth.value.toDouble(), maxHeight.value.toDouble())
                    val fit = ScreenViewport(ScreenSize(display.width, display.height), view).let { it.frame(it.displayBounds) }
                    ScreenVideo(s, fit, Modifier.fillMaxSize())
                    // Pixels for the picture's size; the session sends them again once its channel opens.
                    val density = LocalDensity.current.density
                    LaunchedEffect(s, view) { s.view(null, ScreenSize(view.width * density, view.height * density)) }
                    SessionNotice(s, compact = true)
                }
                // Waiting for a tap (below); or the full viewer has the screen, or the app is in the background.
                available -> Unit
                else -> Unavailable(state, compact = true)
            }
            PlayNotice(prompt.takeIf { available }, compact = true) { prompt = null }
            AnimatedVisibility(live && agent != null, Modifier.align(Alignment.TopStart).padding(8.dp),
                enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
                AgentAtScreen(agent?.name.orEmpty())
            }
            AnimatedVisibility(live, Modifier.align(Alignment.BottomEnd).padding(8.dp), enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
                Box(Modifier.size(28.dp).clip(CircleShape).background(SCRIM), contentAlignment = Alignment.Center) {
                    Symbol("arrow.up.left.and.arrow.down.right", size = 14.dp, tint = Color.White)
                }
            }
        }
    }
}

/** Dark glass over the picture, under its open button. */
private val SCRIM = Color.Black.copy(alpha = 0.55f)
