package com.codync.android

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.pm.ActivityInfo
import android.content.res.Configuration
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.clickable
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.Text
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import com.codync.android.core.*
import com.codync.android.design.*

/**
 * The computer's screen, full screen on the phone (CodyncUI `ScreenView`). Opened from Computers
 * (to use the computer) or from a bot's chat (to watch it work, then take over).
 */
@Composable internal fun ScreenView(state: AppState, store: AppStore, watching: String?, close: () -> Unit) {
    val screen = state.screen
    val agent = screen?.agentBot?.let { id -> state.bots.firstOrNull { it.id == id } }
    var session by remember { mutableStateOf<ScreenSession?>(null) }
    var display by remember { mutableStateOf<ScreenDisplay?>(null) }
    var interactive by remember { mutableStateOf(false) }
    var mode by remember { mutableStateOf(TouchMode.Trackpad) }
    var keyboard by remember { mutableStateOf(false) }
    var armed by remember { mutableStateOf(emptyList<String>()) }
    var zoomed by remember { mutableStateOf(false) }
    var resetToken by remember { mutableIntStateOf(0) }
    var tookOver by remember { mutableStateOf(false) }
    val portrait = LocalConfiguration.current.orientation != Configuration.ORIENTATION_LANDSCAPE

    fun setInteractive(on: Boolean) {
        val allowed = on && state.screen?.input == true
        interactive = allowed
        if (allowed != tookOver) {
            tookOver = allowed
            store.screenTakeover(allowed)
        }
        if (!allowed) keyboard = false
    }
    /** Also runs when the screen becomes available after opening. */
    fun start() {
        val current = state.screen
        if (session != null || current == null || !current.available) return
        val d = display ?: current.mainDisplay ?: return
        display = d
        session = ScreenSession(store, d).also { it.start() }
        // Watching a bot: look first. Otherwise take the screen (bots may still look).
        setInteractive(watching == null)
    }
    fun stop() {
        session?.close()
        session = null
        if (tookOver) store.screenTakeover(false)
        tookOver = false
    }
    // In the background the stream stops and the bots get the computer back, as if the viewer closed.
    LifecycleEventEffect(Lifecycle.Event.ON_START) { start() }
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) { stop() }
    DisposableEffect(Unit) { onDispose { stop() } }
    LaunchedEffect(screen?.available) { if (screen?.available == true && session == null) start() }
    LaunchedEffect(screen?.input) { if (screen?.input != true) setInteractive(false) }
    BackHandler(onBack = close)
    FullScreenWindow()
    KeyboardDismissal(keyboard) { keyboard = false }

    CompositionLocalProvider(LocalDark provides true, LocalContentColor provides Color.White) {
        val bar = remember { mutableIntStateOf(0) }
        val barHeight = with(LocalDensity.current) { bar.intValue.toDp() }
        val showBar = keyboard && interactive
        GlassStage(Modifier.fillMaxSize().background(Color.Black), chrome = {
            TopBar(state, session, display, agent, interactive, mode, keyboard, zoomed, close,
                toggleKeyboard = { keyboard = !keyboard },
                toggleMode = { mode = if (mode == TouchMode.Trackpad) TouchMode.Direct else TouchMode.Trackpad },
                fit = { resetToken += 1 },
                pick = { d -> display = d; session?.switchDisplay(d); resetToken += 1 },
                takeOver = { setInteractive(!interactive) })
            AnimatedVisibility(showBar, Modifier.align(Alignment.BottomCenter).imePadding().onSizeChanged { bar.intValue = it.height },
                enter = slideInVertically(Motion.layoutOffset) { it } + fadeIn(Motion.fade()),
                exit = slideOutVertically(Motion.layoutOffset) { it } + fadeOut(Motion.fade())) {
                ModifierBar(armed, { armed = it }) { key -> session?.send(ScreenRequests.key(key, armed)); armed = emptyList() }
            }
        }) {
            val s = session
            val d = display
            if (s != null && d != null && screen?.available == true) {
                // Portrait: the screen moves above the keyboard. Landscape: the keyboard overlays it,
                // since shrinking there would leave the screen too small.
                ScreenCanvas(s, d, mode, interactive, keyboard, armed, { armed = it }, { zoomed = it }, resetToken,
                    if (portrait) Modifier.imePadding().padding(bottom = if (showBar) barHeight else 0.dp) else Modifier)
                Status(s, screen)
            } else Unavailable(state)
        }
    }
}

/** The status and navigation bars step aside while the screen is open; a swipe shows them for a moment. */
@Composable private fun FullScreenWindow() {
    val activity = LocalActivity.current ?: return
    val view = LocalView.current
    DisposableEffect(activity) {
        // The Rotate button's choice lasts only while the screen is open.
        val orientation = activity.requestedOrientation
        val controller = WindowCompat.getInsetsController(activity.window, view)
        controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        controller.hide(WindowInsetsCompat.Type.systemBars())
        val light = controller.isAppearanceLightStatusBars
        controller.isAppearanceLightStatusBars = false
        onDispose {
            activity.requestedOrientation = orientation
            controller.show(WindowInsetsCompat.Type.systemBars())
            controller.isAppearanceLightStatusBars = light
        }
    }
}

/** Hiding the soft keyboard with Back also turns the keyboard button off. */
@OptIn(ExperimentalLayoutApi::class)
@Composable private fun KeyboardDismissal(keyboard: Boolean, dismissed: () -> Unit) {
    val visible = WindowInsets.isImeVisible
    var seen by remember { mutableStateOf(false) }
    LaunchedEffect(keyboard, visible) {
        if (!keyboard) seen = false
        else if (visible) seen = true
        else if (seen) { seen = false; dismissed() }
    }
}

@Composable private fun TopBar(state: AppState, session: ScreenSession?, display: ScreenDisplay?, agent: Bot?,
    interactive: Boolean, mode: TouchMode, keyboard: Boolean, zoomed: Boolean, close: () -> Unit, toggleKeyboard: () -> Unit,
    toggleMode: () -> Unit, fit: () -> Unit, pick: (ScreenDisplay) -> Unit, takeOver: () -> Unit) {
    val activity = LocalActivity.current
    Row(Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.displayCutout.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))
        .padding(horizontal = 12.dp).padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OverlayButton("Close", "xmark", action = close)
        AnimatedVisibility(agent != null && !interactive, Modifier.weight(1f, fill = false), enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
            Row(Modifier.clip(CircleShape).frost(20.dp, MATERIAL).padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Symbol("cursorarrow.motionlines", size = 14.dp, tint = Color.White)
                Text("${agent?.name.orEmpty()} is using the computer", style = Type.caption.medium, color = Color.White,
                    maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Spacer(Modifier.weight(1f))
        // An explicit rotation works even with the phone's auto-rotate off.
        OverlayButton("Rotate", "rotate.right") { activity?.let(::rotate) }
        if (session != null) {
            if (interactive) {
                OverlayButton(if (keyboard) "Hide keyboard" else "Keyboard", if (keyboard) "keyboard.chevron.compact.down" else "keyboard", action = toggleKeyboard)
                OverlayButton(if (mode == TouchMode.Trackpad) "Touch mode: trackpad" else "Touch mode: direct",
                    if (mode == TouchMode.Trackpad) "rectangle.and.hand.point.up.left" else "hand.point.up.left", action = toggleMode)
                ClipboardMenu(session)
            }
            AnimatedVisibility(zoomed, enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
                OverlayButton("Fit screen", "arrow.down.right.and.arrow.up.left", action = fit)
            }
            val displays = state.screen?.displays.orEmpty()
            if (displays.size > 1) {
                DropdownMenu({ displays.map { d -> MenuItem(d.name, icon = "display", selected = d.id == display?.id) { pick(d) } } },
                    label = "Displays") { IconLabel("display.2") }
            }
            OverlayButton(if (interactive) "Hand back to bots" else "Take over", if (interactive) "hand.raised.slash" else "hand.raised",
                enabled = state.screen?.input == true, action = takeOver)
        }
    }
}

/** Flip between portrait and landscape; leaving the viewer gives rotation back to the phone. */
private fun rotate(activity: android.app.Activity) {
    val landscape = activity.resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
    activity.requestedOrientation = if (landscape) ActivityInfo.SCREEN_ORIENTATION_PORTRAIT else ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
}

/** Menu items have no disabled state on iOS: unavailable actions are left out instead. */
@Composable private fun ClipboardMenu(session: ScreenSession) {
    val clipboard = LocalContext.current.getSystemService(ClipboardManager::class.java)
    DropdownMenu({
        buildList {
            // The description says whether there's text without reading it (which Android announces).
            if (clipboard.primaryClipDescription?.hasMimeType(ClipDescription.MIMETYPE_TEXT_PLAIN) == true) {
                add(MenuItem("Send my clipboard", icon = "arrow.up.doc.on.clipboard") {
                    clipboard.primaryClip?.getItemAt(0)?.text?.toString()?.let(session::pushClipboard)
                })
            }
            if (session.remoteClipboard != null) {
                add(MenuItem("Copy from computer", icon = "arrow.down.doc.on.clipboard") {
                    session.takeRemoteClipboard()?.let { clipboard.setPrimaryClip(ClipData.newPlainText("Codync", it)) }
                })
            }
        }
    }, label = "Clipboard") { IconLabel(if (session.remoteClipboard == null) "doc.on.clipboard" else "doc.on.clipboard.fill") }
}

@Composable private fun BoxScope.Status(session: ScreenSession, screen: ScreenState) {
    when (val phase = session.phase) {
        ScreenSession.Phase.Connecting -> Notice(null, "Connecting to the screen…")
        ScreenSession.Phase.Reconnecting -> Notice(null, "Reconnecting…")
        is ScreenSession.Phase.Failed -> Notice("exclamationmark.triangle", phase.message) { session.start() }
        ScreenSession.Phase.Live, ScreenSession.Phase.Closed -> {
            val text = if (!screen.input) "View only. To allow control, open Codync → Settings → Computer access on the computer." else session.lastError
            if (text != null) Text(text, Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = 24.dp)
                .padding(horizontal = 16.dp).clip(RoundedCornerShape(if (screen.input) 8.dp else 12.dp)).frost(20.dp, MATERIAL)
                .padding(if (screen.input) 8.dp else 12.dp), style = Type.caption, color = Color.White)
        }
    }
}

@Composable private fun BoxScope.Unavailable(state: AppState) {
    val screen = state.screen
    when {
        state.connection !is LinkState.Ready -> Notice("wifi.slash", "This computer is offline.")
        screen == null -> Notice("arrow.down.circle", "Update Codync on your computer to use its screen from here.")
        else -> Notice("lock.display", screen.accessGuidance)
    }
}

/** A centered icon (or a spinner), a line and an optional retry. */
@Composable private fun BoxScope.Notice(icon: String?, text: String, retry: (() -> Unit)? = null) {
    Column(Modifier.align(Alignment.Center).widthIn(max = 420.dp).padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (icon != null) Symbol(icon, size = 40.dp, tint = Palette.secondary) else Spinner(22.dp)
        Text(text, style = Type.callout, color = Palette.secondary, textAlign = TextAlign.Center)
        if (retry != null) SecondaryButton(retry) { Symbol("arrow.clockwise", size = 20.dp, contentDescription = "Try again") }
    }
}

@Composable private fun OverlayButton(label: String, icon: String, enabled: Boolean = true, action: () -> Unit) {
    val (source, pressed) = rememberPress()
    Box(Modifier.pressScale(pressed).graphicsLayer { alpha = if (enabled) 1f else 0.4f }
        .clickable(source, null, enabled = enabled, role = Role.Button, onClick = action)
        .semantics { contentDescription = label }) { IconLabel(icon) }
}

/** `.ultraThinMaterial` in a 36 pt circle, a white glyph on top. */
@Composable private fun IconLabel(icon: String) {
    Box(Modifier.size(36.dp).clip(CircleShape).frost(20.dp, MATERIAL), contentAlignment = Alignment.Center) {
        Symbol(icon, size = 18.dp, tint = Color.White)
    }
}

/** Keys the phone keyboard lacks. Modifiers arm for the next key or click. */
@Composable private fun ModifierBar(armed: List<String>, arm: (List<String>) -> Unit, press: (String) -> Unit) {
    Row(Modifier.fillMaxWidth().background(Color.Black.copy(alpha = 0.85f)).horizontalScroll(rememberScrollState())
        .padding(horizontal = 12.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        for ((name, icon) in MODIFIERS) {
            val on = name in armed
            BarKey(name, icon, on) { arm(if (on) armed - name else armed + name) }
        }
        Box(Modifier.width(0.5.dp).height(24.dp).background(Color.White.copy(alpha = 0.3f)))
        for ((name, icon) in KEYS) BarKey(name, icon, false) { press(name) }
    }
}

@Composable private fun BarKey(name: String, icon: String, on: Boolean, action: () -> Unit) {
    val (source, pressed) = rememberPress()
    val fill by animateColorAsState(if (on) Color.White else Color.White.copy(alpha = 0.12f), Motion.hover(), label = "Key")
    Box(Modifier.pressScale(pressed).size(40.dp, 34.dp).clip(RoundedCornerShape(8.dp)).background(fill)
        .clickable(source, null, role = Role.Button, onClick = action)
        .semantics { contentDescription = name; selected = on }, contentAlignment = Alignment.Center) {
        Symbol(icon, size = 18.dp, tint = if (on) Color.Black else Color.White)
    }
}

/** `.ultraThinMaterial` over the dark screen. */
private val MATERIAL = Color.Black.copy(alpha = 0.32f)
private val MODIFIERS = listOf("cmd" to "command", "option" to "option", "ctrl" to "control", "shift" to "shift")
private val KEYS = listOf("escape" to "escape", "tab" to "arrow.right.to.line", "left" to "arrow.left", "up" to "arrow.up",
    "down" to "arrow.down", "right" to "arrow.right")
