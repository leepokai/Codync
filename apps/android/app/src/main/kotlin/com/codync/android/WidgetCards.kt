package com.codync.android

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.ProviderLimit
import com.codync.android.core.UsageProvider
import com.codync.android.core.UsageReport
import com.codync.android.core.UsageWindow
import com.codync.android.core.WidgetBot
import com.codync.android.core.usageLimits
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlin.math.roundToInt

// The iOS widget cards (CodyncKit `WidgetCards.swift`), for the State gallery's previews.
// The Home Screen widgets draw the same layouts with Glance.

enum class ProviderLayout { Small, Medium, Large }

/** Claude keeps its orange; everyone else is ink, like the rest of the app. */
/** ACP registry id of the provider's agent, for its logo: Claude and Codex run through ACP adapters. */
internal val UsageProvider.registry: String get() = if (id in listOf("claude", "codex")) "$id-acp" else id

@Composable internal fun UsageProvider.tint(): Color = if (id == "claude") Color(0xFFD97757) else Palette.accent

/** Gauges add Codex's blue so two providers' bars tell apart (CodyncKit `widgetTint`). */
@Composable internal fun UsageProvider.widgetTint(): Color =
    if (id == "codex") (if (LocalDark.current) Color(0xFF70ACDC) else Color(0xFF326FAD)) else tint()

/** "Session" for the 5-hour window, "Opus" for "Weekly · Opus", else the label. */
internal val UsageWindow.title: String get() = when {
    label == "5-hour" -> "Session"
    " · " in label -> label.substringAfter(" · ")
    else -> label
}

/** "resets 3h 20m" / "resets Sep 26 at 12pm", compact for widgets and cards (CodyncKit `resetsShort`). */
internal fun UsageWindow.resetsShort(now: Long = System.currentTimeMillis()): String? {
    resetDate(now)?.let { at -> return if (at > now) "resets " + until(at, now) else null }
    return resetsText?.takeIf(String::isNotBlank)?.let { "resets " + it.replace(Regex(" \\(.*\\)$"), "") }
}

/** "resets 3h 20m" / "resets in 2d 6h". Compact times become "3小时20分后重置". */
@Composable internal fun resetLine(raw: String): String {
    val language = LocalAppLanguage.current.resolved(LocalSystemTag.current)
    return spokenReset(raw, language.usesChinese(LocalSystemChinese.current), translator())
}

internal fun spokenReset(raw: String, chinese: Boolean, tr: (String) -> String): String {
    val time = when {
        raw.startsWith("resets in ") -> raw.removePrefix("resets in ")
        raw.startsWith("resets ") -> raw.removePrefix("resets ")
        else -> return tr(raw)
    }
    if (!chinese) {
        if (tr("resets") == "resets") return raw
        return if (Regex("[0-9]+[dhm]( [0-9]+[dhm])*").matches(time)) tr("resets in {time}").replace("{time}", time)
        else "${tr("resets")} $time"
    }
    val spoken = compactTime(time, tr)
    return if (spoken != null) spoken + tr("after reset") else "${tr("resets")} $time"
}

private fun compactTime(time: String, tr: (String) -> String): String? {
    var spoken = ""
    var matched = false
    for (token in time.split(" ")) {
        if (token.length < 2) return null
        val n = token.dropLast(1).toIntOrNull() ?: return null
        val word = when (token.last()) {
            'd' -> tr("day unit")
            'h' -> tr("hour unit")
            'm' -> tr("minute unit")
            else -> return null
        }
        matched = true
        if (n == 0) continue
        spoken += "$n$word"
    }
    if (!matched) return null
    return spoken.ifEmpty { "0${tr("minute unit")}" }
}

/** Time left: 2d · 3h 20m · 45m (CodyncKit `RelativeTime.until`). */
internal fun until(at: Long, now: Long): String {
    val minutes = maxOf(0L, at - now) / 60_000
    val hours = minutes / 60
    return when { hours >= 48 -> "${hours / 24}d"; hours > 0 -> "${hours}h ${minutes % 60}m"; else -> "${minutes}m" }
}

/** A provider's character, with a red "!" once any of its limits is nearly used up. */
@Composable internal fun ProviderMascot(provider: UsageProvider, size: Dp) {
    val full = (provider.tightest?.percent ?: 0.0) >= 90
    val context = androidx.compose.ui.platform.LocalContext.current
    val revision by WidgetStore.changes.collectAsState()
    val original = remember(revision) { WidgetStore.originalIcons(context) }
    val logo = agentIconRes(provider.registry)
    Box(Modifier.size(size)) {
        if (original && logo != null) androidx.compose.foundation.Image(androidx.compose.ui.res.painterResource(logo), null,
            Modifier.fillMaxSize().padding(size * 0.08f), colorFilter = androidx.compose.ui.graphics.ColorFilter.tint(provider.tint()))
        else CharacterAvatar(if (provider.id == "codex") "hex" else "blob", "gray", size, if (full) "needsInput" else "idle", highlight = provider.tint())
        if (full) Box(Modifier.align(Alignment.TopEnd).offset(x = size * 0.12f, y = -size * 0.12f).size(size * 0.34f).clip(Capsule())
            .background(Palette.danger), contentAlignment = Alignment.Center) {
            Text("!", style = Type.system(size.value * 0.24f, FontWeight.Black), color = Color.White)
        }
    }
}

/** Fine, separated ticks keep low and high usage legible without a heavy gauge. */
@Composable internal fun UsageTicks(percent: Double, tint: Color, height: Dp) {
    val value = if (percent.isFinite()) percent.coerceIn(0.0, 100.0) else 0.0
    val color = if (value >= 90) Palette.danger else tint
    Canvas(Modifier.fillMaxWidth().height(height)) {
        val gap = 1.5.dp.toPx()
        val count = maxOf(1, (size.width / 4.dp.toPx()).toInt())
        val filled = (count * value / 100).roundToInt()
        val width = (size.width - gap * (count - 1)) / count
        for (i in 0 until count) drawRoundRect(if (i < filled) color else color.copy(alpha = 0.16f), Offset(i * (width + gap), 0f),
            Size(width, size.height), CornerRadius(1.dp.toPx()))
    }
}

/** One provider's limits: the tightest in a small widget, two windows in medium, four and a summary in large. */
@Composable internal fun ProviderWidgetCard(provider: UsageProvider, layout: ProviderLayout) {
    val tint = provider.widgetTint()
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(if (layout == ProviderLayout.Small) 6.dp else 8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
            ProviderMascot(provider, 22.dp)
            Text(provider.name, Modifier.weight(1f), style = Type.system(13f).semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (layout != ProviderLayout.Small) Text(tr("Usage"), style = Type.system(10f).medium, color = Palette.secondary)
        }
        val top = provider.tightest
        if (layout == ProviderLayout.Small && top != null) {
            Spacer(Modifier.weight(1f))
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(tr(top.title), style = Type.system(11f), color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Row(verticalAlignment = Alignment.Bottom) {
                    val ink = if ((top.percent ?: 0.0) >= 90) Palette.danger else Palette.text
                    Text("${(top.percent ?: 0.0).roundToInt()}", style = Type.system(32f).semibold.monospacedDigit, color = ink)
                    Text("%", Modifier.padding(bottom = 4.dp), style = Type.system(17f).medium, color = ink)
                }
            }
            UsageTicks(top.percent ?: 0.0, tint, 13.dp)
            val reset = top.resetsShort()
            Text(if (reset == null) tr("Usage reported by your computer") else resetLine(reset), style = Type.system(9f), color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        } else {
            if (layout == ProviderLayout.Large && top != null) Row(Modifier.padding(vertical = 4.dp), verticalAlignment = Alignment.Bottom,
                horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("${(top.percent ?: 0.0).roundToInt()}%", style = Type.system(36f).semibold.monospacedDigit,
                    color = if ((top.percent ?: 0.0) >= 90) Palette.danger else Palette.text)
                Text("${tr(top.title)} · ${tr("highest usage")}", Modifier.padding(bottom = 6.dp), style = Type.system(11f), color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            for (window in provider.limits.take(if (layout == ProviderLayout.Large) 4 else 2)) {
                Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(tr(window.title), Modifier.weight(1f), style = Type.system(11f).medium, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text("${(window.percent ?: 0.0).roundToInt()}%", style = Type.system(13f).semibold.monospacedDigit,
                            color = if ((window.percent ?: 0.0) >= 90) Palette.danger else Palette.text)
                    }
                    UsageTicks(window.percent ?: 0.0, tint, 12.dp)
                    val reset = window.resetsShort()
                    if (reset != null) Text(resetLine(reset), Modifier.fillMaxWidth(), style = Type.system(9f), color = Palette.secondary,
                        maxLines = 1, overflow = TextOverflow.Ellipsis, textAlign = TextAlign.End)
                }
            }
        }
    }
}

/** Every provider's limits: the tightest is the headline, the rest thin bars that take color near the limit (iOS `UsageWidgetView`). */
@Composable internal fun UsageLimitsCard(report: UsageReport, wide: Boolean) {
    val limits = usageLimits(report)
    val top = limits.firstOrNull() ?: return
    val rest = limits.drop(1)
    if (wide) Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(18.dp)) {
        Box(Modifier.width(118.dp)) { LimitHero(top) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(9.dp)) { for (limit in rest.take(4)) LimitBar(limit) }
    } else Column(Modifier.fillMaxSize()) {
        LimitHero(top)
        Spacer(Modifier.weight(1f).heightIn(min = 6.dp))
        Column(verticalArrangement = Arrangement.spacedBy(7.dp)) { for (limit in rest.take(2)) LimitBar(limit) }
    }
}

@Composable private fun LimitHero(limit: ProviderLimit) {
    val percent = limit.window.percent ?: 0.0
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text("${limit.provider} · ${limit.window.label}", style = Type.caption, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text("${percent.roundToInt()}%", style = Type.system(34f).semibold.monospacedDigit, color = if (percent >= 90) Palette.danger else Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        val reset = limit.window.resetsShort()
        Text(if (reset == null) " " else resetLine(reset), style = Type.caption2, color = Palette.tertiary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable private fun LimitBar(limit: ProviderLimit) {
    val percent = limit.window.percent ?: 0.0
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row {
            Text("${limit.provider} ${limit.window.label}", Modifier.weight(1f), style = Type.caption2, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("${percent.roundToInt()}%", style = Type.caption2.monospacedDigit, color = Palette.secondary)
        }
        UsageTicks(percent, usageTint(percent), 10.dp)
    }
}

/** "How full is this limit" (CodyncKit `Palette.usageTint`). */
@Composable private fun usageTint(percent: Double): Color = if (percent >= 90) Palette.danger else if (percent >= 70) Palette.warning else Palette.accent

/** The state word the widgets show and its color: only what needs you (or failed) stands out. */
@Composable internal fun widgetState(status: String): Pair<String, Color> = when (status) {
    "needsInput" -> "Needs you" to Palette.attention
    "error" -> "Failed" to Palette.danger
    "working" -> "Working" to Palette.text
    else -> "Ready" to Palette.secondary
}

private fun rank(bot: WidgetBot) = when (bot.status) { "needsInput" -> 0; "working" -> 1; else -> 2 }

/** Who needs you and who's still working: a count, then the bots in that order. */
@Composable internal fun BotsWidgetCard(bots: List<WidgetBot>, activities: Map<String, String>, wide: Boolean, large: Boolean) {
    val ordered = bots.sortedBy(::rank)
    val needs = bots.count { it.status == "needsInput" }
    val working = bots.count { it.status == "working" }
    val count = if (needs > 0) needs else if (working > 0) working else bots.size
    val label = if (needs > 0) (if (needs == 1) "Needs you" else "Need you") else if (working > 0) "Working" else if (bots.isEmpty()) "No bots yet" else "All quiet"
    val summary: @Composable () -> Unit = {
        Column(verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text("$count", style = Type.system(36f).semibold.monospacedDigit, color = if (needs > 0) Palette.attention else Palette.text)
            Text(tr(label), style = Type.system(12f).medium, color = Palette.secondary)
        }
    }
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Bots", Modifier.weight(1f), style = Type.system(13f).semibold, color = Palette.text)
            Symbol("bubble.left.and.bubble.right", size = 15.dp, tint = Palette.secondary)
        }
        when {
            large -> {
                Row(verticalAlignment = Alignment.Bottom) {
                    Box(Modifier.weight(1f)) { summary() }
                    Text("${bots.size} total · $working working", style = Type.system(11f), color = Palette.secondary)
                }
                Hairline()
                BotRows(ordered.take(6), activities)
            }
            wide -> Row(Modifier.fillMaxHeight(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                Box(Modifier.width(76.dp)) { summary() }
                Box(Modifier.width(1.dp).fillMaxHeight().background(Palette.border))
                Box(Modifier.weight(1f)) { BotRows(ordered.take(3), activities) }
            }
            else -> {
                Spacer(Modifier.weight(1f))
                summary()
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Row(horizontalArrangement = Arrangement.spacedBy((-4).dp)) {
                        for (bot in ordered.take(3)) CharacterAvatar(bot.avatarShape, bot.avatarColor, 23.dp)
                    }
                    Spacer(Modifier.weight(1f))
                    if (working > 0 && needs > 0) Text("$working working", style = Type.system(9f), color = Palette.secondary)
                }
            }
        }
    }
}

@Composable private fun BotRows(bots: List<WidgetBot>, activities: Map<String, String>) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for (bot in bots) {
            val (label, tint) = widgetState(bot.status)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                CharacterAvatar(bot.avatarShape, bot.avatarColor, 24.dp)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                    Text(widgetName(bot), style = Type.system(11f).semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    val activity = activities[bot.id]?.takeIf { bot.status == "working" || bot.status == "needsInput" }
                    if (activity != null) Text(tr(activity), style = Type.system(10f), color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Text(tr(label), style = Type.system(10f, if (bot.status == "needsInput") FontWeight.SemiBold else FontWeight.Normal), color = tint, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        if (bots.isEmpty()) Text("Create your first bot in Codync.", style = Type.system(12f), color = Palette.secondary)
    }
}

/** Four bots in a 2×2 grid, each in its mood with a dot for its state; empty spots sleep. */
@Composable internal fun BotsTeamCard(slots: List<WidgetBot?>) {
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for (row in slots.chunked(2)) Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (bot in row) Box(Modifier.weight(1f).fillMaxHeight().clip(RoundedRectangle(16.dp))
                .background(if (bot == null) Palette.bubbleAgent.copy(alpha = 0.5f) else Palette.bubbleAgent), contentAlignment = Alignment.Center) {
                if (bot == null) Box(Modifier.size(26.dp)) { CharacterAvatar("blob", "gray", 26.dp, monochrome = Palette.text.copy(alpha = 0.18f)) }
                else {
                    Column(Modifier.padding(horizontal = 4.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        CharacterAvatar(bot.avatarShape, bot.avatarColor, 30.dp, bot.status)
                        Text(widgetName(bot), style = Type.system(9f).semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    if (bot.status == "working" || bot.status == "error") Box(Modifier.align(Alignment.TopEnd).padding(7.dp).size(7.dp)
                        .clip(Capsule()).background(widgetState(bot.status).second))
                }
            }
        }
    }
}

/** Sample bots in the gallery have translated names. A bot the user named stays as they wrote it. */
@Composable internal fun widgetName(bot: WidgetBot): String =
    if (bot.id.startsWith("preview-")) tr(bot.name) else bot.name

/** Illustrative data only, for previews labelled as samples (the iOS gallery's). */
internal object WidgetPreview {
    val bots = listOf(WidgetBot("preview-review", "Reviewer", "needsInput", avatarColor = "green", avatarShape = "blob"),
        WidgetBot("preview-build", "Builder", "working", avatarColor = "orange", avatarShape = "squircle"),
        WidgetBot("preview-docs", "Docs", "idle", avatarColor = "gray", avatarShape = "hex"))
    val activities = mapOf("preview-review" to "Review changes", "preview-build" to "Running tests")
    val usage = UsageReport(listOf(
        UsageProvider("claude", "Claude", listOf(UsageWindow("five_hour", "5-hour", 16.0, resetsText = "3h 20m"),
            UsageWindow("seven_day", "Weekly", 59.0, resetsText = "2d 6h")), source = "claude"),
        UsageProvider("codex", "Codex", listOf(UsageWindow("primary", "5-hour", 8.0, resetsText = "2h 10m"),
            UsageWindow("secondary", "Weekly", 27.0, resetsText = "4d 2h")), source = "sessions")))
}
