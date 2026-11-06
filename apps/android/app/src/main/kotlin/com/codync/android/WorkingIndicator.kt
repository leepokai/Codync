package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonObject

/** The bot's current activity and elapsed time; with thinking `details` on (Settings), a tap unfolds the turn's steps. */
@Composable internal fun WorkingIndicator(bot: Bot, steps: List<Entry>, details: Boolean) {
    val needs = bot.status == "needsInput"
    // Each turn starts folded.
    var expanded by remember(bot.startedAt) { mutableStateOf(false) }
    val open = details && expanded
    val turn by animateFloatAsState(if (open) 90f else 0f, Motion.layout(), label = "Chevron")
    val action = tr(if (open) "Hide steps" else "Show steps")
    val state = tr(if (open) "Expanded" else "Collapsed")
    val toggle = if (!details) Modifier
        else Modifier.clickable(role = Role.Button, onClickLabel = action) { expanded = !expanded }.semantics { stateDescription = state }
    Column(Modifier.fillMaxWidth().padding(top = 6.dp, end = 40.dp)) {
        Column(Modifier.then(if (open) Modifier.fillMaxWidth() else Modifier).animateContentSize(Motion.layoutSize)
            .clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent).padding(horizontal = 16.dp, vertical = 12.dp)) {
            Row(Modifier.then(if (open) Modifier.fillMaxWidth() else Modifier).then(toggle),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                ThinkingOrb(if (needs) ThinkingOrbState.Listening else ThinkingOrbState.Working, 16.dp, if (needs) Palette.warning else Palette.secondary)
                Text(tr(bot.activity.ifEmpty { "Working…" }), Modifier.weight(1f, fill = open).shimmerEffect(active = !needs), style = Type.subheadline,
                    color = if (needs) Palette.warning else Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                bot.startedAt?.let { Elapsed(it) }
                if (details) Symbol("chevron.right", Modifier.rotate(turn), size = 13.dp, tint = Palette.tertiary)
            }
            AnimatedVisibility(open, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
                exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) { WorkingSteps(steps) }
        }
    }
}

/** The turn's thinking, narration, tool calls and plan: hugs them, scrolls past 280 dp, and follows new steps until
 *  the reader scrolls up. A turn can have none yet: Claude's thinking text arrives as its thinking ends. */
@Composable private fun WorkingSteps(steps: List<Entry>) {
    val scroll = rememberScrollState()
    var follow by remember { mutableStateOf(true) }
    LaunchedEffect(scroll.isScrollInProgress) { if (!scroll.isScrollInProgress) follow = !scroll.canScrollForward }
    // Follows new steps once they're laid out, not a row the reader unfolds (that grows the box too).
    LaunchedEffect(steps) {
        if (!follow) return@LaunchedEffect
        withFrameNanos {}
        scroll.scrollTo(scroll.maxValue)
    }
    Column(Modifier.padding(top = 12.dp).heightIn(max = 280.dp).verticalScroll(scroll), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (steps.isEmpty()) Text(tr("Nothing yet."), style = Type.body, color = Palette.tertiary)
        for (entry in steps) key(entry.id) { TraceRow(entry, unfolded = true) }
    }
}

/** `Text(date, style: .timer)`: the time since `startedAt`, every second. */
@Composable private fun Elapsed(startedAt: Long) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(startedAt) { while (true) { now = System.currentTimeMillis(); delay(1000) } }
    val seconds = maxOf(0, (now - startedAt) / 1000)
    Text(if (seconds >= 3600) "%d:%02d:%02d".format(seconds / 3600, seconds / 60 % 60, seconds % 60) else "%d:%02d".format(seconds / 60, seconds % 60),
        style = Type.footnote.monospacedDigit, color = Palette.tertiary)
}

/** Document conversations share the richer working-step disclosure. */
@Composable internal fun WorkingIndicator(bot: Bot, thinking: String?) {
    val steps = if (thinking == null) emptyList() else listOf(Entry("thinking", 0, bot.id, 0, "thought", 0,
        JsonObject(mapOf("text" to kotlinx.serialization.json.JsonPrimitive(thinking)))))
    WorkingIndicator(bot, steps, thinking != null)
}
