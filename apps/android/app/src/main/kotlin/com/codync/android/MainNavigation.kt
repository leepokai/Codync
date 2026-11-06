package com.codync.android

import android.content.Context
import android.view.accessibility.AccessibilityManager
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
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.core.VoicePhase
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** The four root destinations of the primary navigation shell. */
internal enum class AppTab(val title: String, val icon: String, val iconSize: Dp = 26.dp) {
    Bots("Bots", "bubble.left.and.bubble.right.fill", 30.dp),
    State("State", "rectangle.stack.fill", 25.dp),
    Extensions("Extensions", "square.grid.2x2", 25.dp),
    Computers("PC", "desktopcomputer", 25.dp),
}

/** Presentation summary for navigation badges across the four destinations. */
internal data class NavigationBadges(
    val bots: TabBadge? = null,
    val state: TabBadge? = null,
    val extensions: TabBadge? = null,
    val computers: TabBadge? = null,
) {
    fun forTab(tab: AppTab): TabBadge? = when (tab) {
        AppTab.Bots -> bots
        AppTab.State -> state
        AppTab.Extensions -> extensions
        AppTab.Computers -> computers
    }
}

/**
 * Computes navigation badges from visible conversations and confirmed host connection.
 * Individual bots shared across multiple group chats are deduplicated by `id`, and cached
 * offline state never announces live work or pending approvals.
 */
internal fun navigationBadges(state: AppState): NavigationBadges {
    val visible = state.bots.filter { !it.deleted && !it.hidden }
    val unread = visible.sumOf { it.unread.coerceAtLeast(0) }
    val botsBadge = if (unread > 0) {
        TabBadge(
            count = unread,
            tone = TabBadgeTone.Accent,
            accessibilityLabel = "$unread unread",
        )
    } else null

    val online = state.actualConnection is LinkState.Ready
    val stateBadge = if (online) {
        val visibleGroupMembers = visible.asSequence()
            .filter { it.kind == "group" }
            .flatMap { it.members.asSequence() }
            .toSet()
        // Unique non-group bots that are visible directly or participate in a visible group chat.
        val uniqueBots = state.bots.asSequence()
            .filter { !it.deleted && it.kind != "group" && (!it.hidden || it.id in visibleGroupMembers) }
            .distinctBy(Bot::id)
            .toList()
        val needsInput = uniqueBots.count { it.status == "needsInput" }
        val working = uniqueBots.count { it.status == "working" }
        when {
            needsInput > 0 -> {
                val label = if (working > 0) {
                    "$needsInput ${if (needsInput == 1) "needs" else "need"} your approval, $working working"
                } else {
                    "$needsInput ${if (needsInput == 1) "needs" else "need"} your approval"
                }
                TabBadge(count = needsInput, tone = TabBadgeTone.Warning, pulse = true, accessibilityLabel = label)
            }
            working > 0 -> TabBadge(count = working, tone = TabBadgeTone.Working, pulse = false, accessibilityLabel = "$working working")
            else -> null
        }
    } else null

    val computersBadge = when {
        state.computers.isEmpty() -> null
        state.actualConnection is LinkState.Ready -> TabBadge(
            tone = TabBadgeTone.Online,
            accessibilityLabel = connectionDescription(state),
        )
        state.connection is LinkState.ComputerOffline || state.connection is LinkState.Failed -> TabBadge(
            tone = TabBadgeTone.Offline,
            accessibilityLabel = connectionDescription(state),
        )
        else -> null
    }

    return NavigationBadges(
        bots = botsBadge,
        state = stateBadge,
        computers = computersBadge,
    )
}

/**
 * Shared scroll visibility state for the bottom navigation bar with threshold and hysteresis.
 * Never hides when screen readers are active, when the bar holds focus, when critical actions
 * are pending, or during programmatic semantics scroll (`!touching`).
 */
internal class BarScrollVisibility(
    private val hideThresholdPx: Float = 168f,
    private val showThresholdPx: Float = 84f,
) {
    var visible by mutableStateOf(true)
        private set
    var barFocused by mutableStateOf(false)
    var touching by mutableStateOf(false)
    var canAutoHide by mutableStateOf(true)
    private var accumulatedDelta = 0f

    val nestedScroll = object : NestedScrollConnection {
        override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
            if (source == NestedScrollSource.UserInput && touching) onScrollDelta(consumed.y)
            return Offset.Zero
        }
    }

    fun onScrollDelta(dy: Float) {
        if (!canAutoHide || barFocused) {
            show()
            return
        }
        if (dy == 0f) return
        // Reset accumulated delta on direction change for smooth hysteresis without jitter.
        if ((dy < 0f && accumulatedDelta > 0f) || (dy > 0f && accumulatedDelta < 0f)) {
            accumulatedDelta = 0f
        }
        accumulatedDelta += dy
        if (visible && accumulatedDelta <= -hideThresholdPx) {
            visible = false
            accumulatedDelta = 0f
        } else if (!visible && accumulatedDelta >= showThresholdPx) {
            visible = true
            accumulatedDelta = 0f
        }
    }

    fun show() {
        visible = true
        accumulatedDelta = 0f
    }
}

/** Whether a screen reader / touch exploration is active on the device. */
@Composable internal fun rememberTouchExplorationActive(): Boolean {
    val context = LocalContext.current
    val manager = remember(context) { context.getSystemService(AccessibilityManager::class.java) }
    var active by remember { mutableStateOf(manager?.isEnabled == true && manager.isTouchExplorationEnabled) }
    DisposableEffect(manager) {
        if (manager == null) return@DisposableEffect onDispose {}
        val a11yListener = AccessibilityManager.AccessibilityStateChangeListener {
            active = it && manager.isTouchExplorationEnabled
        }
        val touchListener = AccessibilityManager.TouchExplorationStateChangeListener {
            active = manager.isEnabled && it
        }
        manager.addAccessibilityStateChangeListener(a11yListener)
        manager.addTouchExplorationStateChangeListener(touchListener)
        onDispose {
            manager.removeAccessibilityStateChangeListener(a11yListener)
            manager.removeTouchExplorationStateChangeListener(touchListener)
        }
    }
    return active
}

/** Whether the usable window dimensions support the expanded NavigationRail + Master-Detail layout. */
internal fun isExpandedWindow(maxWidth: Dp, maxHeight: Dp): Boolean =
    maxWidth >= 600.dp && maxHeight >= 360.dp

/** Device-scoped navigation preferences (bar style and scroll auto-hide). */
internal object NavigationPreferences {
    private const val PREFS_NAME = "navigation-preferences"
    private const val KEY_STYLE = "bar_style"
    private const val KEY_AUTO_HIDE = "auto_hide_scroll"

    private val mutableStyle = MutableStateFlow(NavBarStyle.FloatingCapsule)
    private val mutableAutoHide = MutableStateFlow(true)
    private var initialized = false

    val style: StateFlow<NavBarStyle> = mutableStyle.asStateFlow()
    val autoHide: StateFlow<Boolean> = mutableAutoHide.asStateFlow()

    @Synchronized fun ensureLoaded(context: Context) {
        if (initialized) return
        val prefs = context.applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        mutableStyle.value = NavBarStyle.fromId(prefs.getString(KEY_STYLE, NavBarStyle.FloatingCapsule.id))
        mutableAutoHide.value = prefs.getBoolean(KEY_AUTO_HIDE, true)
        initialized = true
    }

    fun setStyle(context: Context, value: NavBarStyle) {
        ensureLoaded(context)
        mutableStyle.value = value
        context.applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            .edit().putString(KEY_STYLE, value.id).apply()
    }

    fun setAutoHide(context: Context, enabled: Boolean) {
        ensureLoaded(context)
        mutableAutoHide.value = enabled
        context.applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            .edit().putBoolean(KEY_AUTO_HIDE, enabled).apply()
    }
}

/**
 * Persistent mini-bar for an active voice call and/or remote computer screen session when the user
 * navigates outside the conversation. Reuses `store::muteVoice`, `store::endVoice`, `store::openBot`
 * and `store::openScreen` without creating or destroying sessions on tab switches.
 */
@Composable internal fun SessionMiniBar(
    state: AppState,
    store: AppStore,
    showVoice: Boolean,
    onOpenVoiceBot: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val call = state.voice?.takeIf { showVoice }
    val online = state.actualConnection is LinkState.Ready
    val screenBotId = state.screen?.agentBot?.takeIf { online && state.screenRequest == null }
    val screenBot = screenBotId?.let { id -> state.bots.firstOrNull { it.id == id && !it.deleted } }

    Column(modifier.fillMaxWidth().padding(horizontal = 14.dp), verticalArrangement = Arrangement.spacedBy(6.dp),
        horizontalAlignment = Alignment.CenterHorizontally) {
        AnimatedVisibility(
            visible = screenBot != null,
            enter = fadeIn(Motion.fade()) + slideInVertically(Motion.layoutOffset) { it / 2 },
            exit = fadeOut(Motion.fade()) + slideOutVertically(Motion.layoutOffset) { it / 2 },
        ) {
            if (screenBot != null) {
                RemoteScreenMiniPill(screenBot) { store.openScreen(screenBot.id) }
            }
        }
        AnimatedVisibility(
            visible = call != null,
            enter = fadeIn(Motion.fade()) + slideInVertically(Motion.layoutOffset) { it / 2 },
            exit = fadeOut(Motion.fade()) + slideOutVertically(Motion.layoutOffset) { it / 2 },
        ) {
            if (call != null) {
                VoiceMiniPill(call, state, store, online, onOpenVoiceBot)
            }
        }
    }
}

@Composable private fun RemoteScreenMiniPill(bot: Bot, watch: () -> Unit) {
    val reduced = reduceMotion()
    val pulse by rememberInfiniteTransition(label = "Screen operating").animateFloat(
        initialValue = 1f,
        targetValue = if (reduced) 1f else 0.45f,
        animationSpec = infiniteRepeatable(tween(900), RepeatMode.Reverse),
        label = "Screen pulse",
    )
    Row(
        Modifier.widthIn(max = 420.dp).glass(Capsule()).plainButton(action = watch)
            .padding(start = 12.dp, end = 12.dp, top = 7.dp, bottom = 7.dp)
            .semantics { contentDescription = "${bot.name} is using the computer. Tap to watch screen." },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Symbol("desktopcomputer", Modifier.graphicsLayer { alpha = pulse }, size = 16.dp, tint = Palette.text)
        Text("${bot.name} is using the computer", Modifier.weight(1f, fill = false), style = Type.caption.semibold,
            color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text("Watch", style = Type.caption.semibold, color = Palette.accent)
    }
}

@Composable private fun VoiceMiniPill(
    call: VoiceCallViewState,
    state: AppState,
    store: AppStore,
    online: Boolean,
    onOpenVoiceBot: () -> Unit,
) {
    val voice = call.state
    val bot = state.bots.firstOrNull { it.id == call.botId }
    val color = bot?.let { AvatarPalette.tint(it.avatarColor) } ?: Palette.accent
    val speaking = voice.phase == VoicePhase.Speaking
    val error = voice.error
    val statusLine = when {
        !online -> "Offline · reconnecting…"
        error != null -> error
        voice.phase == VoicePhase.Interrupted -> "Paused · tap to return"
        else -> callStatusText(voice, bot?.status == "working")
    }
    val statusInk = when {
        !online || voice.phase == VoicePhase.Interrupted -> Palette.warning
        voice.error != null || voice.phase == VoicePhase.Failed -> Palette.danger
        else -> Palette.secondary
    }
    Row(
        Modifier.widthIn(max = 440.dp).fillMaxWidth().glass(Capsule())
            .padding(start = 10.dp, end = 6.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(
            Modifier.weight(1f).plainButton {
                onOpenVoiceBot()
                store.openBot(call.botId)
            }.semantics(mergeDescendants = true) {
                contentDescription = "Return to voice call with ${bot?.name ?: "bot"}, $statusLine"
            },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(9.dp),
        ) {
            if (bot != null) BotAvatar(bot, state.bots, 32.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                Text(bot?.name ?: "Voice call", style = Type.subheadline.semibold, color = Palette.text,
                    maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(statusLine, style = Type.caption2, color = statusInk, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            CallLevelDots(voice.level, speaking && online, bot?.status == "working" && online, voice.phase == VoicePhase.Starting && online, color,
                Modifier.widthIn(min = 32.dp, max = 52.dp).height(16.dp))
        }
        Box(
            Modifier.size(38.dp).clip(Capsule())
                .background(if (voice.muted) Palette.accentFill else Palette.background)
                .plainButton(action = store::muteVoice)
                .semantics { contentDescription = if (voice.muted) "Unmute" else "Mute" },
            contentAlignment = Alignment.Center,
        ) {
            Symbol(if (voice.muted) "mic.slash" else "mic", size = 18.dp,
                tint = if (voice.muted) Palette.onAccent else Palette.text)
        }
        Box(
            Modifier.size(38.dp).clip(Capsule()).background(Palette.danger)
                .plainButton { store.endVoice() }
                .semantics { contentDescription = "End call" },
            contentAlignment = Alignment.Center,
        ) {
            Symbol("xmark", size = 16.dp, tint = Color.White)
        }
    }
}
