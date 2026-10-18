package com.codync.android

import androidx.compose.animation.Crossfade
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import com.kyant.shapes.UnevenRoundedRectangle
import kotlin.math.abs
import kotlin.math.pow

/**
 * "Add a widget", acted out on a drawn Android phone (iOS `WidgetSetupDemo`): hold the Home Screen,
 * Widgets, find Codync, pick a widget, Add. Every frame is a pure function of one clock, so it
 * loops, pauses and scrubs. `active` stops the clock while the State tab is hidden.
 */
@Composable internal fun WidgetSetupDemo(active: Boolean) {
    val still = reduceMotion()
    var playing by remember { mutableStateOf(!still) }
    var scrubbing by remember { mutableStateOf(false) }
    var t by remember { mutableFloatStateOf(if (still) 6.6f else 0f) }
    val running = playing && !scrubbing && active
    LaunchedEffect(running) {
        if (!running) return@LaunchedEffect
        var last = 0L
        while (true) withFrameNanos { now ->
            if (last != 0L) t = (t + (now - last) / 1e9f) % DURATION
            last = now
        }
    }
    val step = step(t)
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Box(Modifier.semantics(mergeDescendants = true) { contentDescription = "How to add the Codync widget"; stateDescription = step.text }) {
            DemoPhone(t)
        }
        Crossfade(step.index, animationSpec = Motion.fade(), label = "Step") { index ->
            Text(STEPS[index].text, Modifier.fillMaxWidth().clearAndSetSemantics {}, style = Type.footnote.medium, color = Palette.text,
                textAlign = TextAlign.Center)
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            IconButton(if (playing) "Pause" else "Play", if (playing) "pause.fill" else "play.fill") { playing = !playing }
            Scrubber(t, scrubbing, Modifier.weight(1f), { scrubbing = it }, { t = it }) { index -> playing = false; t = STEPS[index].start + 0.05f }
        }
    }
}

/** The timeline: drag anywhere on it to scrub; TalkBack steps through the script instead. */
@Composable private fun Scrubber(t: Float, scrubbing: Boolean, modifier: Modifier, scrub: (Boolean) -> Unit, seek: (Float) -> Unit,
    jump: (Int) -> Unit) {
    val index = step(t).index
    BoxWithConstraints(modifier.height(32.dp)
        .pointerInput(Unit) {
            awaitEachGesture {
                val down = awaitFirstDown()
                scrub(true)
                seek((down.position.x / size.width).coerceIn(0f, 0.999f) * DURATION)
                while (true) {
                    val change = awaitPointerEvent().changes.firstOrNull() ?: break
                    if (!change.pressed) break
                    seek((change.position.x / size.width).coerceIn(0f, 0.999f) * DURATION)
                    change.consume()
                }
                scrub(false)
            }
        }
        .semantics {
            contentDescription = "Step"; stateDescription = STEPS[index].text
            customActions = listOfNotNull(
                if (index < STEPS.lastIndex) CustomAccessibilityAction("Next step") { jump(index + 1); true } else null,
                if (index > 0) CustomAccessibilityAction("Previous step") { jump(index - 1); true } else null,
            )
        }, contentAlignment = Alignment.CenterStart) {
        val x = maxWidth * (t / DURATION)
        val knob = if (scrubbing) 18.dp else 14.dp
        Box(Modifier.fillMaxWidth().height(4.dp).clip(Capsule()).background(Palette.accentDim))
        Box(Modifier.width(x).height(4.dp).clip(Capsule()).background(Palette.accent))
        Box(Modifier.offset(x = x - knob / 2).size(knob).clip(Capsule()).background(Palette.accent))
    }
}

// MARK: - script

private class DemoStep(val index: Int, val start: Float, val text: String)

private const val DURATION = 9.5f

private val STEPS = listOf(
    DemoStep(0, 0f, "Touch and hold an empty spot on your Home Screen"),
    DemoStep(1, 1.8f, "Tap Widgets"),
    DemoStep(2, 2.6f, "Find Codync in the list"),
    DemoStep(3, 4.5f, "Tap a widget, then Add"),
    DemoStep(4, 6.0f, "It's on your Home Screen. Touch and hold it to move it."),
)

private fun step(t: Float) = STEPS.last { t >= it.start }

/** 0 before `a`, 1 after `b`, smooth in between. */
private fun ramp(t: Float, a: Float, b: Float): Float {
    val x = ((t - a) / (b - a)).coerceIn(0f, 1f)
    return x * x * (3 - 2 * x)
}

/** Ease-out for things that arrive (sheets, pages). */
private fun arrive(t: Float, a: Float, b: Float): Float = 1 - (1 - ((t - a) / (b - a)).coerceIn(0f, 1f)).pow(3)

// MARK: - the phone

private val PHONE_W = 184.dp
private val PHONE_H = 384.dp
private const val SHEET_TOP = 40f

// Where the finger goes (screen points) and when it taps.
private val PATH = listOf(
    0.2f to Offset(150f, 300f), 0.9f to Offset(128f, 250f), 1.8f to Offset(128f, 250f),
    2.2f to Offset(104f, 212f), 2.5f to Offset(104f, 212f), 3.4f to Offset(92f, 193f),
    4.1f to Offset(92f, 193f), 4.6f to Offset(92f, 258f), 5.0f to Offset(92f, 258f),
    5.45f to Offset(92f, 318f), 6.3f to Offset(92f, 318f),
)
private val TAPS = listOf(2.35f, 4.0f, 4.85f, 5.75f)

private fun pressed(t: Float, tap: Float) = abs(t - tap) < 0.12f

@Composable private fun DemoPhone(t: Float) {
    val menu = ramp(t, 1.65f, 1.85f) * (1 - ramp(t, 2.4f, 2.6f))
    val sheet = arrive(t, 2.5f, 3.0f) * (1 - ramp(t, 5.8f, 6.2f))
    val placed = arrive(t, 5.95f, 6.4f)
    // The demo is a dark phone in both modes, as the iOS one.
    CompositionLocalProvider(LocalDark provides true) {
        Box(Modifier.clip(RoundedRectangle(35.dp)).background(Color.Black).padding(5.dp)) {
            Box(Modifier.size(PHONE_W, PHONE_H).clip(RoundedRectangle(30.dp))) {
                Wallpaper(sheet)
                HomeScreen(t, sheet, placed)
                LongPressMenu(t, menu)
                WidgetPicker(t, sheet)
                Finger(t)
                StatusBar()
            }
        }
    }
}

@Composable private fun Wallpaper(sheet: Float) {
    Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color(0xFF243B6B), Color(0xFF5B4A8B), Color(0xFF2A2340)))))
    Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.35f * sheet)))
}

@Composable private fun StatusBar() {
    Box(Modifier.fillMaxWidth().height(24.dp).padding(horizontal = 16.dp), contentAlignment = Alignment.Center) {
        // The punch-hole camera.
        Box(Modifier.size(9.dp).clip(Capsule()).background(Color.Black))
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text("9:41", style = Type.system(9f, FontWeight.SemiBold), color = Color.White)
            Spacer(Modifier.weight(1f))
            Symbol("wifi", size = 10.dp, tint = Color.White)
            Symbol("battery.75percent", size = 11.dp, tint = Color.White)
        }
    }
}

private val APPS = listOf("calendar" to 0xFFE5534B, "photo" to 0xFFF0A030, "clock" to 0xFF4A4F57, "cloud" to 0xFF3D8BFD,
    "music.note" to 0xFFE0558C, "folder" to 0xFF2E9E6A, "globe" to 0xFF2D7FF9, "gearshape" to 0xFF6B7280,
    "bell" to 0xFFF2B33D, "terminal" to 0xFF222831, "doc.text" to 0xFF3E7BD6, "key" to 0xFF8A63D2)
private val DOCK = listOf("text.bubble" to 0xFF2E9E6A, "safari" to 0xFF2D7FF9, "mic" to 0xFFE5534B, "photo.on.rectangle" to 0xFFF0A030)
private const val WIDGET_H = 84f

@Composable private fun HomeScreen(t: Float, sheet: Float, placed: Float) {
    val press = ramp(t, 1.0f, 1.6f) * (1 - ramp(t, 1.75f, 1.95f))
    Column(Modifier.fillMaxSize().graphicsLayer { val s = 1 - 0.04f * sheet - 0.02f * press; scaleX = s; scaleY = s }
        .padding(top = 30.dp, start = 14.dp, end = 14.dp, bottom = 10.dp)) {
        Text("Sat, Oct 17", style = Type.system(11f, FontWeight.Medium), color = Color.White.copy(alpha = 0.92f))
        Spacer(Modifier.height(6.dp))
        // The icons below slide down as the widget takes its row.
        Box(Modifier.fillMaxWidth().height(((WIDGET_H + 8f) * placed).dp), contentAlignment = Alignment.TopCenter) {
            Box(Modifier.graphicsLayer { alpha = placed; val s = 0.9f + 0.1f * placed; scaleX = s; scaleY = s; transformOrigin = TransformOrigin(0.5f, 0f) }) {
                WidgetThumb(156.dp, WIDGET_H.dp, Color(0xFF1B1B1F))
            }
        }
        for (row in APPS.chunked(4)) {
            Row(Modifier.fillMaxWidth().padding(bottom = 9.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                for ((symbol, tint) in row) AppIcon(symbol, tint, label = true)
            }
        }
        Spacer(Modifier.weight(1f))
        Row(Modifier.fillMaxWidth().padding(bottom = 10.dp), horizontalArrangement = Arrangement.SpaceBetween) {
            for ((symbol, tint) in DOCK) AppIcon(symbol, tint, label = false)
        }
        Row(Modifier.fillMaxWidth().height(24.dp).clip(Capsule()).background(Color.White.copy(alpha = 0.2f)).padding(horizontal = 10.dp),
            verticalAlignment = Alignment.CenterVertically) {
            Symbol("magnifyingglass", size = 11.dp, tint = Color.White.copy(alpha = 0.8f))
            Spacer(Modifier.weight(1f))
            Symbol("mic", size = 11.dp, tint = Color.White.copy(alpha = 0.8f))
        }
        // The gesture bar.
        Box(Modifier.fillMaxWidth().padding(top = 9.dp), contentAlignment = Alignment.Center) {
            Box(Modifier.size(56.dp, 3.dp).clip(Capsule()).background(Color.White.copy(alpha = 0.8f)))
        }
    }
}

@Composable private fun AppIcon(symbol: String, tint: Long, label: Boolean) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Box(Modifier.size(32.dp).clip(Capsule()).background(Color(tint)), contentAlignment = Alignment.Center) {
            Symbol(symbol, size = 16.dp, tint = Color.White)
        }
        if (label) Box(Modifier.size(20.dp, 3.dp).clip(Capsule()).background(Color.White.copy(alpha = 0.55f)))
    }
}

/** The Bots widget at a small scale: the same card the Home Screen gets. */
@Composable private fun WidgetThumb(width: Dp, height: Dp, fill: Color) {
    val scale = 0.6f
    Box(Modifier.size(width, height), contentAlignment = Alignment.Center) {
        Box(Modifier.requiredSize(width / scale, height / scale).graphicsLayer { scaleX = scale; scaleY = scale }
            .clip(RoundedRectangle(22.dp)).background(fill).padding(12.dp)) {
            BotsWidgetCard(WidgetPreview.bots, WidgetPreview.activities, wide = true, large = false)
        }
    }
}

/** The launcher's long-press menu, growing from the finger. */
@Composable private fun LongPressMenu(t: Float, shown: Float) {
    if (shown <= 0f) return
    Column(Modifier.offset(56.dp, 176.dp).width(112.dp)
        .graphicsLayer { alpha = shown; val s = 0.7f + 0.3f * shown; scaleX = s; scaleY = s; transformOrigin = TransformOrigin(0.65f, 1f) }
        .clip(RoundedRectangle(14.dp)).background(Color(0xFF2B2930))) {
        MenuRow("Wallpaper & style", "paintbrush", false)
        MenuRow("Widgets", "square.grid.2x2", pressed(t, 2.35f))
        MenuRow("Home settings", "gearshape", false)
    }
}

@Composable private fun MenuRow(title: String, symbol: String, highlighted: Boolean) {
    Row(Modifier.fillMaxWidth().height(24.dp).background(Color.White.copy(alpha = if (highlighted) 0.14f else 0f)).padding(horizontal = 9.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
        Symbol(symbol, size = 11.dp, tint = Color.White)
        Text(title, style = Type.system(9f, FontWeight.Medium), color = Color.White, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** The system widget picker: a sheet of apps, Codync opening to its widgets, then Add. */
@Composable private fun WidgetPicker(t: Float, sheet: Float) {
    if (sheet <= 0f) return
    val open = arrive(t, 3.95f, 4.35f)
    val chosen = arrive(t, 4.85f, 5.1f)
    val height = PHONE_H - SHEET_TOP.dp
    Column(Modifier.offset(y = SHEET_TOP.dp + height * (1 - sheet)).size(PHONE_W, height)
        .clip(UnevenRoundedRectangle(topStart = 20.dp, topEnd = 20.dp)).background(Color(0xFF1F1E24))) {
        Box(Modifier.fillMaxWidth().padding(top = 6.dp, bottom = 6.dp), contentAlignment = Alignment.Center) {
            Box(Modifier.size(28.dp, 3.dp).clip(Capsule()).background(Color.White.copy(alpha = 0.3f)))
        }
        Text("Widgets", Modifier.fillMaxWidth(), style = Type.system(12f, FontWeight.SemiBold), color = Color.White, textAlign = TextAlign.Center)
        Row(Modifier.padding(horizontal = 12.dp, vertical = 8.dp).fillMaxWidth().height(22.dp).clip(Capsule()).background(Color.White.copy(alpha = 0.1f))
            .padding(horizontal = 9.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            Symbol("magnifyingglass", size = 10.dp, tint = Color.White.copy(alpha = 0.5f))
            Text("Search widgets", style = Type.system(9f), color = Color.White.copy(alpha = 0.5f))
        }
        PickerRow("Calendar", "calendar", 0xFFE5534B, "2 widgets")
        PickerRow("Clock", "clock", 0xFF4A4F57, "4 widgets")
        PickerRow("Codync", null, 0, "3 widgets", highlighted = pressed(t, 4.0f), open = open)
        // Codync's widgets, unfolded under its row.
        Column(Modifier.fillMaxWidth().height((96f * open + 40f * open * chosen).dp).graphicsLayer { alpha = open },
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            // The picked widget lifts a little; Add appears under it.
            Box(Modifier.graphicsLayer { val s = (if (pressed(t, 4.85f)) 0.96f else 1f) + 0.04f * chosen; scaleX = s; scaleY = s }) {
                WidgetThumb(150.dp, 78.dp, Color(0xFF2C2B32))
            }
            if (chosen > 0f) Box(Modifier.graphicsLayer { alpha = chosen; val s = if (pressed(t, 5.75f)) 0.94f else 1f; scaleX = s; scaleY = s }
                .size(84.dp, 26.dp).clip(Capsule()).background(Color(0xFFD0BCFF)), contentAlignment = Alignment.Center) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    Symbol("plus", size = 11.dp, tint = Color(0xFF381E72))
                    Text("Add", style = Type.system(10f, FontWeight.SemiBold), color = Color(0xFF381E72))
                }
            }
        }
        PickerRow("Files", "folder", 0xFF2E9E6A, "1 widget")
        PickerRow("Weather", "cloud", 0xFF3D8BFD, "3 widgets")
    }
}

@Composable private fun PickerRow(title: String, symbol: String?, tint: Long, count: String, highlighted: Boolean = false, open: Float = 0f) {
    Row(Modifier.fillMaxWidth().height(30.dp).background(Color.White.copy(alpha = if (highlighted) 0.1f else 0f)).padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (symbol != null) Box(Modifier.size(20.dp).clip(Capsule()).background(Color(tint)), contentAlignment = Alignment.Center) {
            Symbol(symbol, size = 11.dp, tint = Color.White)
        } else Box(Modifier.size(20.dp).clip(Capsule()).background(Color(0xFF2C2B32)), contentAlignment = Alignment.Center) {
            CharacterAvatar("hex", "gray", 16.dp)
        }
        Column(Modifier.weight(1f)) {
            Text(title, style = Type.system(10f, FontWeight.Medium), color = Color.White, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(count, style = Type.system(8f), color = Color.White.copy(alpha = 0.5f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Symbol("chevron.down", Modifier.graphicsLayer { rotationZ = 180f * open }, size = 10.dp, tint = Color.White.copy(alpha = 0.5f))
    }
}

// MARK: - finger

@Composable private fun Finger(t: Float) {
    val visible = ramp(t, 0.15f, 0.4f) * (1 - ramp(t, 6.3f, 6.6f))
    if (visible <= 0f) return
    val tapDepth = TAPS.maxOf { maxOf(0f, 1 - abs(t - it) / 0.14f) }
    val holding = ramp(t, 0.95f, 1.05f) * (1 - ramp(t, 1.65f, 1.75f))
    val hold = ramp(t, 1.0f, 1.6f)
    val p = fingerPoint(t)
    Box(Modifier.offset((p.x - 17f).dp, (p.y - 17f).dp).size(34.dp).graphicsLayer { alpha = visible }, contentAlignment = Alignment.Center) {
        // The long press fills a ring before the menu opens.
        Canvas(Modifier.fillMaxSize()) {
            val line = 2.dp.toPx()
            drawArc(Color.White.copy(alpha = 0.8f), -90f, 360f * hold * holding, false, style = Stroke(line, cap = StrokeCap.Round),
                topLeft = Offset(line / 2, line / 2), size = androidx.compose.ui.geometry.Size(size.width - line, size.height - line))
        }
        Box(Modifier.size(24.dp).graphicsLayer { val s = 1 - 0.2f * maxOf(tapDepth, holding); scaleX = s; scaleY = s }
            .clip(Capsule()).background(Color.White.copy(alpha = 0.35f + 0.25f * maxOf(tapDepth, holding)))
            .border(1.5.dp, Color.White.copy(alpha = 0.85f), Capsule()))
    }
}

private fun fingerPoint(t: Float): Offset {
    val next = PATH.indexOfFirst { it.first > t }
    if (next == -1) return PATH.last().second
    if (next == 0) return PATH.first().second
    val (ta, a) = PATH[next - 1]
    val (tb, b) = PATH[next]
    val k = ramp(t, ta, tb)
    return Offset(a.x + (b.x - a.x) * k, a.y + (b.y - a.y) * k)
}
