package com.codync.android.design

import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import kotlin.math.exp

/**
 * The iOS text styles at the default Dynamic Type size, set in Inter (SF Pro is Apple-only).
 * Sizes are `sp`, so Android's font scale grows them like Dynamic Type.
 */
object Type {
    val largeTitle = style(34f, 41f)
    val title = style(28f, 34f)
    val title2 = style(22f, 28f)
    val title3 = style(20f, 25f)
    val headline = style(17f, 22f, FontWeight.SemiBold)
    val body = style(17f, 22f)
    val callout = style(16f, 21f)
    val subheadline = style(15f, 20f)
    val footnote = style(13f, 18f)
    val caption = style(12f, 16f)
    val caption2 = style(11f, 13f)

    /** `Font.system(size:weight:)`. */
    fun system(size: Float, weight: FontWeight = FontWeight.Normal) = style(size, size * 1.2f, weight)

    val monospaced = FontFamily.Monospace
}

val TextStyle.medium get() = copy(fontWeight = FontWeight.Medium)
val TextStyle.semibold get() = copy(fontWeight = FontWeight.SemiBold)
val TextStyle.bold get() = copy(fontWeight = FontWeight.Bold)
val TextStyle.monospacedDigit get() = copy(fontFeatureSettings = "tnum")
val TextStyle.monospaced get() = copy(fontFamily = FontFamily.Monospace, letterSpacing = 0.sp)

private val families = HashMap<Int, FontFamily>()

/** Inter's optical size follows the text size, as SF switches from Text to Display. */
private fun family(size: Float): FontFamily = families.getOrPut(size.toInt().coerceIn(14, 32)) {
    val opsz = size.coerceIn(14f, 32f)
    FontFamily(listOf(FontWeight.Normal, FontWeight.Medium, FontWeight.SemiBold, FontWeight.Bold).map { weight ->
        Font(R.font.inter, weight, variationSettings = FontVariation.Settings(
            FontVariation.weight(weight.weight), FontVariation.Setting("opsz", opsz)))
    })
}

/** Inter's dynamic tracking (rsms.me/inter/dynmetrics), tightened toward SF Pro's widths. */
private fun tracking(size: Float): TextUnit = (-0.0223f + 0.185f * exp(-0.1745f * size) - 0.008f).em

private fun style(size: Float, line: Float, weight: FontWeight = FontWeight.Normal) = TextStyle(
    fontFamily = family(size), fontSize = size.sp, lineHeight = line.sp, fontWeight = weight,
    letterSpacing = tracking(size),
    lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
)
