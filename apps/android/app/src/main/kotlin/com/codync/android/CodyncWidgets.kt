package com.codync.android

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.net.Uri
import android.os.Build
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import com.codync.android.design.translate
import androidx.glance.LocalSize
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.appWidgetBackground
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.appwidget.updateAll
import androidx.glance.background
import androidx.glance.color.ColorProvider
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.ContentScale
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxHeight
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextAlign
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider as Ink
import com.codync.android.core.ProviderLimit
import com.codync.android.core.UsageProvider
import com.codync.android.core.UsageWindow
import com.codync.android.core.WidgetBot
import com.codync.android.core.WidgetFeed
import com.codync.android.core.WidgetKind
import com.codync.android.core.teamSlots
import com.codync.android.core.usageLimits
import com.codync.android.design.characterAvatarBitmap
import kotlin.math.roundToInt

// The Home Screen widgets, drawn like the iOS widgets (CodyncKit `WidgetCards.swift`) and the
// State tab's previews: Provider usage, Usage limits, Bots and Team.

class ProviderWidget : CodyncWidget(WidgetKind.Provider)
class UsageWidget : CodyncWidget(WidgetKind.Usage)
class BotsWidget : CodyncWidget(WidgetKind.Bots)
class TeamWidget : CodyncWidget(WidgetKind.Team)

abstract class CodyncWidget(private val kind: WidgetKind) : GlanceAppWidget() {
    override val sizeMode = SizeMode.Responsive(setOf(SMALL, MEDIUM, LARGE))
    override val previewSizeMode = sizeMode
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val widget = GlanceAppWidgetManager(context).getAppWidgetId(id)
        provideContent {
            val revision by WidgetStore.changes.collectAsState()
            val feed = remember(revision) { WidgetStore.feed(context) }
            val provider = remember(revision) { WidgetStore.provider(context, widget) }
            val team = remember(revision) { WidgetStore.team(context, widget) }
            androidx.compose.runtime.key(revision) {
                WidgetCard(feed, kind, provider, team, sample = false)
            }
        }
    }
    override suspend fun providePreview(context: Context, widgetCategory: Int) {
        provideContent { WidgetCard(sampleFeed(), kind, "claude", emptyList(), sample = true) }
    }

    private companion object {
        val SMALL = DpSize(110.dp, 110.dp)
        val MEDIUM = DpSize(250.dp, 110.dp)
        val LARGE = DpSize(250.dp, 250.dp)
    }
}

private val ink = ColorProvider(day = Color(0xFF141414), night = Color(0xFFF2F2F2))
private val secondary = ColorProvider(day = Color(0xFF6B6B6B), night = Color(0xFF9A9A9A))
private val tertiary = ColorProvider(day = Color(0xFF767676), night = Color(0xFF929292))
private val attention = ColorProvider(day = Color(0xFF936000), night = Color(0xFFECAF52))
private val danger = ColorProvider(day = Color(0xFFC23A2B), night = Color(0xFFF0A7A7))
private val tile = ColorProvider(day = Color(0xFFF0F0F0), night = Color(0xFF1C1C1C))
private val divider = ColorProvider(day = Color(0xFFE6E6E6), night = Color(0xFF262626))
private val ground = ColorProvider(day = Color(0xFFFFFFFF), night = Color(0xFF141414))

private fun style(size: Float, color: Ink, weight: FontWeight = FontWeight.Normal) = TextStyle(color, size.sp, weight)

@Composable private fun WidgetCard(feed: WidgetFeed, kind: WidgetKind, providerId: String, team: List<String?>, sample: Boolean) {
    val context = LocalContext.current
    val size = LocalSize.current
    val layout = if (size.height >= 230.dp) ProviderLayout.Large else if (size.width >= 230.dp) ProviderLayout.Medium else ProviderLayout.Small
    // Usage limits opens the app as it is, as on iOS.
    val open = widgetIntent(context, feed, usage = kind == WidgetKind.Provider)
    Box(GlanceModifier.fillMaxSize().appWidgetBackground().background(ground).cornerRadius(22.dp)
        .then(if (sample) GlanceModifier else GlanceModifier.clickable(actionStartActivity(open))).padding(14.dp)) {
        when {
            kind == WidgetKind.Usage -> UsageCard(context, feed, wide = layout != ProviderLayout.Small, sample)
            feed.computerId == null && !sample -> Empty("Codync", "Connect a computer in Codync.")
            kind == WidgetKind.Provider -> {
                val provider = feed.usage.providers.firstOrNull { it.id == providerId }
                if (provider == null) Empty(providerName(providerId), "No usage yet. Open Codync to refresh.")
                else ProviderCard(context, provider, layout)
            }
            kind == WidgetKind.Team -> TeamCard(context, feed, team, sample)
            else -> BotsCard(context, feed, layout, sample)
        }
    }
}

private fun providerName(id: String) = when (id) { "claude" -> "Claude"; "codex" -> "Codex"; else -> id }

@Composable private fun Empty(title: String, message: String) {
    Column(GlanceModifier.fillMaxSize()) {
        Text(title, style = style(13f, ink, FontWeight.Medium), maxLines = 1)
        Spacer(GlanceModifier.defaultWeight())
        Text(message, style = style(12f, secondary), maxLines = 3)
    }
}

/** Claude keeps its orange; Codex gets its blue in gauges; everyone else is ink. */
private fun gaugeTint(context: Context, id: String): Int = when (id) {
    "claude" -> 0xFFD97757.toInt()
    "codex" -> if (dark(context)) 0xFF70ACDC.toInt() else 0xFF326FAD.toInt()
    else -> ink.getColor(context).toArgb()
}

private fun dark(context: Context) = context.resources.configuration.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK ==
    android.content.res.Configuration.UI_MODE_NIGHT_YES

@Composable private fun ProviderCard(context: Context, provider: UsageProvider, layout: ProviderLayout) {
    val tint = gaugeTint(context, provider.id)
    val top = provider.tightest
    Column(GlanceModifier.fillMaxSize()) {
        Row(GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            val logo = agentIconRes(provider.registry)
            if (WidgetStore.originalIcons(context) && logo != null) Image(ImageProvider(logo), provider.name, GlanceModifier.size(22.dp),
                colorFilter = ColorFilter.tint(if (provider.id == "claude") ColorProvider(day = Color(0xFFD97757), night = Color(0xFFD97757)) else ink))
            else Face(context, if (provider.id == "codex") "hex" else "blob", "gray", 22.dp,
                highlight = if (provider.id == "claude") 0xFFD97757.toInt() else null)
            Spacer(GlanceModifier.width(7.dp))
            Text(provider.name, GlanceModifier.defaultWeight(), style = style(13f, ink, FontWeight.Medium), maxLines = 1)
            if (layout != ProviderLayout.Small) Text(phrase("Usage"), style = style(10f, secondary, FontWeight.Medium))
        }
        if (layout == ProviderLayout.Small && top != null) {
            Spacer(GlanceModifier.defaultWeight())
            Text(phrase(top.title), style = style(11f, secondary), maxLines = 1)
            Row(verticalAlignment = Alignment.Bottom) {
                val color = if ((top.percent ?: 0.0) >= 90) danger else ink
                Text("${(top.percent ?: 0.0).roundToInt()}", style = style(32f, color, FontWeight.Medium))
                Text("%", GlanceModifier.padding(bottom = 4.dp), style = style(17f, color, FontWeight.Medium))
            }
            Ticks(context, top.percent ?: 0.0, tint, 13.dp)
            Spacer(GlanceModifier.height(6.dp))
            Text(top.resetsShort()?.let { resetSpoken(it) } ?: phrase("Usage reported by your computer"), style = style(9f, secondary), maxLines = 1)
        } else {
            Spacer(GlanceModifier.height(8.dp))
            if (layout == ProviderLayout.Large && top != null) {
                Row(GlanceModifier.padding(vertical = 4.dp), verticalAlignment = Alignment.Bottom) {
                    Text("${(top.percent ?: 0.0).roundToInt()}%", style = style(36f, if ((top.percent ?: 0.0) >= 90) danger else ink, FontWeight.Medium))
                    Spacer(GlanceModifier.width(8.dp))
                    Text("${phrase(top.title)} · ${phrase("highest usage")}", GlanceModifier.padding(bottom = 6.dp), style = style(11f, secondary), maxLines = 2)
                }
            }
            for (window in provider.limits.take(if (layout == ProviderLayout.Large) 4 else 2)) WindowRow(context, window, tint)
        }
    }
}

@Composable private fun WindowRow(context: Context, window: UsageWindow, tint: Int) {
    Column(GlanceModifier.fillMaxWidth().padding(bottom = 6.dp)) {
        Row(GlanceModifier.fillMaxWidth()) {
            Text(phrase(window.title), GlanceModifier.defaultWeight(), style = style(11f, secondary, FontWeight.Medium), maxLines = 1)
            Text("${(window.percent ?: 0.0).roundToInt()}%", style = style(13f, if ((window.percent ?: 0.0) >= 90) danger else ink, FontWeight.Medium))
        }
        Spacer(GlanceModifier.height(3.dp))
        Ticks(context, window.percent ?: 0.0, tint, 12.dp)
        window.resetsShort()?.let { Text(resetSpoken(it), GlanceModifier.fillMaxWidth(), style = TextStyle(secondary, 9.sp, textAlign = TextAlign.End), maxLines = 1) }
    }
}

/** Every provider's limits: the tightest is the headline, the rest thin bars that take color near the limit (iOS `UsageWidgetView`). */
@Composable private fun UsageCard(context: Context, feed: WidgetFeed, wide: Boolean, sample: Boolean) {
    val limits = usageLimits(feed.usage)
    val top = limits.firstOrNull()
    if (top == null) {
        Column(GlanceModifier.fillMaxSize()) {
            Face(context, "blob", "gray", 26.dp)
            Spacer(GlanceModifier.defaultWeight())
            Text(phrase(if (feed.computerId == null && !sample) "Open Codync to pair with your computer." else "No usage reported yet."),
                style = style(12f, secondary), maxLines = 3)
        }
        return
    }
    val rest = limits.drop(1)
    if (wide) Row(GlanceModifier.fillMaxSize()) {
        Box(GlanceModifier.width(118.dp)) { LimitHero(top) }
        Spacer(GlanceModifier.width(18.dp))
        Column(GlanceModifier.defaultWeight()) {
            for ((index, limit) in rest.take(4).withIndex()) {
                if (index > 0) Spacer(GlanceModifier.height(9.dp))
                LimitBar(context, limit)
            }
        }
    } else Column(GlanceModifier.fillMaxSize()) {
        LimitHero(top)
        Spacer(GlanceModifier.defaultWeight())
        for ((index, limit) in rest.take(2).withIndex()) {
            if (index > 0) Spacer(GlanceModifier.height(7.dp))
            LimitBar(context, limit)
        }
    }
}

@Composable private fun LimitHero(limit: ProviderLimit) {
    val percent = limit.window.percent ?: 0.0
    Column {
        Text("${limit.provider} · ${phrase(limit.window.title)}", style = style(12f, secondary), maxLines = 1)
        Text("${percent.roundToInt()}%", style = style(34f, if (percent >= 90) danger else ink, FontWeight.Medium), maxLines = 1)
        Text(limit.window.resetsShort()?.let { resetSpoken(it) } ?: " ", style = style(11f, tertiary), maxLines = 1)
    }
}

@Composable private fun LimitBar(context: Context, limit: ProviderLimit) {
    val percent = limit.window.percent ?: 0.0
    Column(GlanceModifier.fillMaxWidth()) {
        Row(GlanceModifier.fillMaxWidth()) {
            Text("${limit.provider} ${phrase(limit.window.title)}", GlanceModifier.defaultWeight(), style = style(11f, secondary), maxLines = 1)
            Text("${percent.roundToInt()}%", style = style(11f, secondary))
        }
        Spacer(GlanceModifier.height(4.dp))
        // Grey until 70%, then amber (iOS `Palette.usageTint`); the ticks turn red from 90%.
        Ticks(context, percent, if (percent >= 70) WARNING else ink.getColor(context).toArgb(), 10.dp)
    }
}

private const val WARNING = 0xFFF0A030.toInt()

@Composable private fun phrase(text: String): String {
    val context = LocalContext.current
    val language = AppLanguageStore.get(context).resolved(context.resources.configuration.locales[0].toLanguageTag())
    val table = AppLanguageStore.phrases(context, language)
    return translate(text, language, language.usesChinese(false), table)
}

@Composable private fun resetSpoken(raw: String): String {
    val context = LocalContext.current
    val language = AppLanguageStore.get(context).resolved(context.resources.configuration.locales[0].toLanguageTag())
    val table = AppLanguageStore.phrases(context, language)
    val systemChinese = language.usesChinese(false)
    return spokenReset(raw, language.usesChinese(systemChinese)) { translate(it, language, systemChinese, table) }
}

/** Fine, separated ticks (iOS `UsageTicks`), as a bitmap: widgets can't draw. */
@Composable private fun Ticks(context: Context, percent: Double, tint: Int, height: Dp) {
    val red = danger.getColor(context).toArgb()
    val bitmap = remember(percent, tint, red) { ticks(percent, tint, red) }
    Image(ImageProvider(bitmap), "${percent.roundToInt()} percent used", GlanceModifier.fillMaxWidth().height(height), contentScale = ContentScale.FillBounds)
}

private fun ticks(percent: Double, tint: Int, red: Int): Bitmap {
    val count = 60; val unit = 8f; val gap = 3f
    val bitmap = Bitmap.createBitmap((count * (unit + gap)).toInt(), 26, Bitmap.Config.ARGB_8888)
    val canvas = Canvas(bitmap)
    val value = if (percent.isFinite()) percent.coerceIn(0.0, 100.0) else 0.0
    val color = if (value >= 90) red else tint
    val filled = (count * value / 100).roundToInt()
    val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    for (i in 0 until count) {
        paint.color = if (i < filled) color else (color and 0x00FFFFFF) or (0x29 shl 24)
        canvas.drawRoundRect(RectF(i * (unit + gap), 0f, i * (unit + gap) + unit, 26f), 2f, 2f, paint)
    }
    return bitmap
}

@Composable private fun Face(context: Context, shape: String, color: String, size: Dp, highlight: Int? = null, customIcon: com.codync.android.core.CustomIcon? = null) {
    val pixels = (size.value * context.resources.displayMetrics.density).toInt().coerceIn(1, 256)
    val inkColor = ink.getColor(context).toArgb()
    val bitmap = remember(shape, color, pixels, inkColor, highlight, customIcon) {
        com.codync.android.design.customAvatarBitmap(customIcon?.dataUrl, customIcon?.tinted == true, pixels,
            com.codync.android.design.AvatarPalette.tint(color).toArgb())
            ?: characterAvatarBitmap(shape, color, pixels, inkColor, highlight)
    }
    Image(ImageProvider(bitmap), null, GlanceModifier.size(size))
}

private fun rank(bot: WidgetBot) = when (bot.status) { "needsInput" -> 0; "working" -> 1; else -> 2 }

private fun stateWord(bot: WidgetBot): Pair<String, Ink> = when (bot.status) {
    "needsInput" -> "Needs you" to attention
    "error" -> "Failed" to danger
    "working" -> "Working" to ink
    else -> "Ready" to secondary
}

/** Who needs you and who's still working: a count, then the bots in that order. */
@Composable private fun BotsCard(context: Context, feed: WidgetFeed, layout: ProviderLayout, sample: Boolean) {
    val bots = feed.bots.sortedBy(::rank)
    val needs = bots.count { it.status == "needsInput" }
    val working = bots.count { it.status == "working" }
    val count = if (needs > 0) needs else if (working > 0) working else bots.size
    val label = if (needs > 0) (if (needs == 1) "Needs you" else "Need you") else if (working > 0) "Working" else if (bots.isEmpty()) "No bots yet" else "All quiet"
    Column(GlanceModifier.fillMaxSize()) {
        Row(GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text("Bots", GlanceModifier.defaultWeight(), style = style(13f, ink, FontWeight.Medium))
            Image(ImageProvider(requireNotNull(com.codync.android.design.symbolRes("bubble.left.and.bubble.right"))), null, GlanceModifier.size(15.dp), colorFilter = ColorFilter.tint(secondary))
        }
        Spacer(GlanceModifier.height(10.dp))
        when (layout) {
            ProviderLayout.Large -> {
                Row(GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.Bottom) {
                    Summary(count, label, needs > 0, GlanceModifier.defaultWeight())
                    Text("${bots.size} total · $working working", style = style(11f, secondary))
                }
                Spacer(GlanceModifier.height(10.dp))
                Box(GlanceModifier.fillMaxWidth().height(1.dp).background(divider)) {}
                Spacer(GlanceModifier.height(10.dp))
                BotRows(context, feed, bots.take(6), sample)
            }
            ProviderLayout.Medium -> Row(GlanceModifier.fillMaxSize(), verticalAlignment = Alignment.CenterVertically) {
                Summary(count, label, needs > 0, GlanceModifier.width(76.dp))
                Spacer(GlanceModifier.width(16.dp))
                Box(GlanceModifier.width(1.dp).fillMaxHeight().background(divider)) {}
                Spacer(GlanceModifier.width(16.dp))
                Box(GlanceModifier.defaultWeight()) { BotRows(context, feed, bots.take(3), sample) }
            }
            ProviderLayout.Small -> {
                Spacer(GlanceModifier.defaultWeight())
                Summary(count, label, needs > 0, GlanceModifier)
                Row(GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    for (bot in bots.take(3)) Face(context, bot.avatarShape, bot.avatarColor, 23.dp, customIcon = bot.customIcon)
                    Spacer(GlanceModifier.defaultWeight())
                    if (working > 0 && needs > 0) Text("$working working", style = style(9f, secondary))
                }
            }
        }
    }
}

@Composable private fun Summary(count: Int, label: String, needs: Boolean, modifier: GlanceModifier) {
    Column(modifier) {
        Text("$count", style = style(36f, if (needs) attention else ink, FontWeight.Medium))
        Text(label, style = style(12f, secondary, FontWeight.Medium), maxLines = 1)
    }
}

@Composable private fun BotRows(context: Context, feed: WidgetFeed, bots: List<WidgetBot>, sample: Boolean) {
    Column(GlanceModifier.fillMaxWidth()) {
        for (bot in bots) {
            val (word, tint) = stateWord(bot)
            val open = if (sample) GlanceModifier else GlanceModifier.clickable(actionStartActivity(widgetIntent(context, feed, bot.id)))
            Row(open.fillMaxWidth().padding(bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Face(context, bot.avatarShape, bot.avatarColor, 24.dp, customIcon = bot.customIcon)
                Spacer(GlanceModifier.width(7.dp))
                Text(bot.name, GlanceModifier.defaultWeight(), style = style(11f, ink, FontWeight.Medium), maxLines = 1)
                Text(word, style = style(10f, tint, if (bot.status == "needsInput") FontWeight.Medium else FontWeight.Normal), maxLines = 1)
            }
        }
        if (bots.isEmpty()) Text("Create your first bot in Codync.", style = style(12f, secondary))
    }
}

/** Four bots in a 2×2 grid, each in its mood with a dot for its state; empty spots sleep. */
@Composable private fun TeamCard(context: Context, feed: WidgetFeed, picks: List<String?>, sample: Boolean) {
    val slots = teamSlots(feed.bots, picks)
    Column(GlanceModifier.fillMaxSize()) {
        for ((index, row) in slots.chunked(2).withIndex()) {
            if (index > 0) Spacer(GlanceModifier.height(8.dp))
            Row(GlanceModifier.fillMaxWidth().defaultWeight()) {
                for ((column, bot) in row.withIndex()) {
                    if (column > 0) Spacer(GlanceModifier.width(8.dp))
                    val open = if (sample || bot == null) GlanceModifier else GlanceModifier.clickable(actionStartActivity(widgetIntent(context, feed, bot.id)))
                    Box(open.defaultWeight().fillMaxHeight().background(tile).cornerRadius(16.dp), contentAlignment = Alignment.Center) {
                        if (bot == null) Face(context, "blob", "gray", 26.dp)
                        else Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            Face(context, bot.avatarShape, bot.avatarColor, 30.dp, customIcon = bot.customIcon)
                            Text(bot.name, style = style(9f, ink, FontWeight.Medium), maxLines = 1)
                        }
                    }
                }
            }
        }
    }
}

internal fun widgetIntent(context: Context, feed: WidgetFeed, bot: String? = null, usage: Boolean = false): Intent {
    val intent = Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
    val computer = feed.computerId ?: return intent
    if (bot != null || usage) intent.data = Uri.Builder().scheme(BuildConfig.URL_SCHEME).authority(if (usage) "usage" else "bot")
        .apply { if (bot != null) appendPath(bot) }.appendQueryParameter("scope", feed.scope).appendQueryParameter("computer", computer).build()
    return intent
}

/** The launcher's picker preview: the State tab's sample data. */
internal fun sampleFeed(): WidgetFeed = WidgetFeed(computerId = "sample", computerName = "Sample computer", bots = WidgetPreview.bots,
    botsAt = System.currentTimeMillis(), usage = WidgetPreview.usage, usageAt = System.currentTimeMillis())

class ProviderWidgetReceiver : CodyncWidgetReceiver() { override val glanceAppWidget = ProviderWidget() }
class UsageWidgetReceiver : CodyncWidgetReceiver() { override val glanceAppWidget = UsageWidget() }
class BotsWidgetReceiver : CodyncWidgetReceiver() { override val glanceAppWidget = BotsWidget() }
class TeamWidgetReceiver : CodyncWidgetReceiver() { override val glanceAppWidget = TeamWidget() }

abstract class CodyncWidgetReceiver : GlanceAppWidgetReceiver() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        super.onUpdate(context, manager, ids)
        WidgetRefreshWorker.schedule(context, WidgetStore.feed(context).computerId != null)
    }
    override fun onDeleted(context: Context, ids: IntArray) { WidgetStore.remove(context, ids); super.onDeleted(context, ids) }
    override fun onDisabled(context: Context) { super.onDisabled(context); WidgetRefreshWorker.schedule(context, WidgetStore.feed(context).computerId != null) }
}

internal object CodyncWidgets {
    val receivers = listOf(ProviderWidgetReceiver::class.java, UsageWidgetReceiver::class.java, BotsWidgetReceiver::class.java,
        TeamWidgetReceiver::class.java)
    fun installed(context: Context): Int = receivers.sumOf { AppWidgetManager.getInstance(context).getAppWidgetIds(ComponentName(context, it)).size }
    suspend fun update(context: Context) {
        if (installed(context) == 0) return
        ProviderWidget().updateAll(context); UsageWidget().updateAll(context); BotsWidget().updateAll(context); TeamWidget().updateAll(context)
    }
    /** The launcher's widget picker shows each widget drawn from sample data (Android 15+); the system rate-limits it. */
    suspend fun publishPreviews(context: Context) {
        if (Build.VERSION.SDK_INT < 35) return
        val manager = GlanceAppWidgetManager(context)
        // A preview that can't be drawn leaves the launcher's placeholder, never a crash at launch.
        try { for (receiver in receivers) manager.setWidgetPreviews(receiver.kotlin) }
        catch (cancelled: kotlinx.coroutines.CancellationException) { throw cancelled }
        catch (error: Exception) { android.util.Log.w("Codync", "Widget previews unavailable", error) }
    }
    fun pin(context: Context, kind: WidgetKind): Boolean {
        val manager = AppWidgetManager.getInstance(context)
        if (!manager.isRequestPinAppWidgetSupported) return false
        val receiver = when (kind) { WidgetKind.Provider -> ProviderWidgetReceiver::class.java; WidgetKind.Usage -> UsageWidgetReceiver::class.java
            WidgetKind.Bots -> BotsWidgetReceiver::class.java; WidgetKind.Team -> TeamWidgetReceiver::class.java }
        return manager.requestPinAppWidget(ComponentName(context, receiver), null, null)
    }
}

