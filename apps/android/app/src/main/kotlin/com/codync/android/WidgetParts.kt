package com.codync.android

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.GlanceModifier
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import androidx.glance.color.ColorProvider
import androidx.glance.layout.ContentScale
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.size
import androidx.glance.text.FontWeight
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider as Ink
import com.codync.android.core.WidgetBot
import com.codync.android.design.characterAvatarBitmap
import com.codync.android.design.translate
import kotlin.math.roundToInt

// What the widgets share: the day and night inks, the translated phrases, the usage ticks and a bot's face.

internal val ink = ColorProvider(day = Color(0xFF141414), night = Color(0xFFF2F2F2))
internal val secondary = ColorProvider(day = Color(0xFF6B6B6B), night = Color(0xFF9A9A9A))
internal val tertiary = ColorProvider(day = Color(0xFF767676), night = Color(0xFF929292))
internal val attention = ColorProvider(day = Color(0xFF936000), night = Color(0xFFECAF52))
internal val danger = ColorProvider(day = Color(0xFFC23A2B), night = Color(0xFFF0A7A7))
internal val tile = ColorProvider(day = Color(0xFFF0F0F0), night = Color(0xFF1C1C1C))
internal val divider = ColorProvider(day = Color(0xFFE6E6E6), night = Color(0xFF262626))
internal val ground = ColorProvider(day = Color(0xFFFFFFFF), night = Color(0xFF141414))

internal fun style(size: Float, color: Ink, weight: FontWeight = FontWeight.Normal) = TextStyle(color, size.sp, weight)

@Composable internal fun phrase(text: String): String {
    val context = LocalContext.current
    val language = AppLanguageStore.get(context).resolved(context.resources.configuration.locales[0].toLanguageTag())
    val table = AppLanguageStore.phrases(context, language)
    return translate(text, language, language.usesChinese(false), table)
}

@Composable internal fun resetSpoken(raw: String): String {
    val context = LocalContext.current
    val language = AppLanguageStore.get(context).resolved(context.resources.configuration.locales[0].toLanguageTag())
    val table = AppLanguageStore.phrases(context, language)
    val systemChinese = language.usesChinese(false)
    return spokenReset(raw, language.usesChinese(systemChinese)) { translate(it, language, systemChinese, table) }
}

/** Fine, separated ticks (iOS `UsageTicks`), as a bitmap: widgets can't draw. */
@Composable internal fun Ticks(context: Context, percent: Double, tint: Int, height: Dp) {
    val red = danger.getColor(context).toArgb()
    val bitmap = remember(percent, tint, red) { ticks(percent, tint, red) }
    Image(ImageProvider(bitmap), "${percent.roundToInt()} percent", GlanceModifier.fillMaxWidth().height(height), contentScale = ContentScale.FillBounds)
}

internal fun ticks(percent: Double, tint: Int, red: Int): Bitmap {
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

@Composable internal fun Face(context: Context, shape: String, color: String, size: Dp, highlight: Int? = null) {
    val pixels = (size.value * context.resources.displayMetrics.density).toInt().coerceIn(1, 256)
    val inkColor = ink.getColor(context).toArgb()
    val bitmap = remember(shape, color, pixels, inkColor, highlight) { characterAvatarBitmap(shape, color, pixels, inkColor, highlight) }
    Image(ImageProvider(bitmap), null, GlanceModifier.size(size))
}


internal fun stateWord(bot: WidgetBot): Pair<String, Ink> = when (bot.status) {
    "needsInput" -> "Needs you" to attention
    "error" -> "Failed" to danger
    "working" -> "Working" to ink
    else -> "Ready" to secondary
}


/** The inks of one widget: they follow day and night, or stay light or dark when the person chose a look. */
internal class WidgetInk(val ink: Ink, val secondary: Ink, val tertiary: Ink, val attention: Ink, val danger: Ink, val tile: Ink,
    val divider: Ink, val ground: Ink, val accent: Ink, val onAccent: Ink)

private fun pair(day: Long, night: Long, theme: com.codync.android.core.WidgetTheme): Ink = when (theme) {
    com.codync.android.core.WidgetTheme.System -> ColorProvider(day = Color(day), night = Color(night))
    com.codync.android.core.WidgetTheme.Light -> ColorProvider(day = Color(day), night = Color(day))
    com.codync.android.core.WidgetTheme.Dark -> ColorProvider(day = Color(night), night = Color(night))
}

/** Same values as the other widgets (`ink`, `secondary`, ...), so every widget reads as one family; contrast stays AA on its ground. */
internal fun widgetInk(theme: com.codync.android.core.WidgetTheme) = WidgetInk(
    ink = pair(0xFF141414, 0xFFF2F2F2, theme), secondary = pair(0xFF6B6B6B, 0xFF9A9A9A, theme), tertiary = pair(0xFF767676, 0xFF929292, theme),
    attention = pair(0xFF936000, 0xFFECAF52, theme), danger = pair(0xFFC23A2B, 0xFFF0A7A7, theme), tile = pair(0xFFF0F0F0, 0xFF1C1C1C, theme),
    divider = pair(0xFFE6E6E6, 0xFF262626, theme), ground = pair(0xFFFFFFFF, 0xFF141414, theme),
    accent = pair(0xFF141414, 0xFFF2F2F2, theme), onAccent = pair(0xFFFFFFFF, 0xFF141414, theme),
)

/** The age of a report in words that need no plural rules: "now", "12 min", "3 h", "2 d". */
internal fun ageWords(minutes: Long): String = when {
    minutes < 1 -> "now"
    minutes < 60 -> "$minutes min"
    minutes < 24 * 60 -> "${minutes / 60} h"
    else -> "${minutes / (24 * 60)} d"
}
