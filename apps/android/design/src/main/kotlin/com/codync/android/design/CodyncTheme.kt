package com.codync.android.design

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Typography
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.Color

/** Keep these pairs aligned with CodyncKit/Design/Theme.swift. */
val botMessageBackground: Color
    @Composable get() = if (LocalDark.current) Color(0xFF15263E) else Color(0xFFEAF2FF)

@Composable
fun CodyncTheme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val colors = if (dark) darkColorScheme(
        primary = Color.White,
        onPrimary = Color.Black,
        secondary = Color.White,
        onSecondary = Color.Black,
        tertiary = Color.White,
        onTertiary = Color.Black,
        background = Color(0xFF0A0A0A),
        onBackground = Color(0xFFF2F2F2),
        surface = Color(0xFF141414),
        onSurface = Color(0xFFF2F2F2),
        onSurfaceVariant = Color(0xFF9A9A9A),
        surfaceVariant = Color(0xFF1C1C1C),
        surfaceContainer = Color(0xFF1C1C1C),
        surfaceContainerLow = Color(0xFF141414),
        surfaceContainerHigh = Color(0xFF1C1C1C),
        surfaceContainerHighest = Color(0xFF3A3A3A),
        surfaceTint = Color.Transparent,
        error = Color(0xFFF0A7A7),
    ) else lightColorScheme(
        primary = Color.Black,
        onPrimary = Color.White,
        secondary = Color.Black,
        onSecondary = Color.White,
        tertiary = Color.Black,
        onTertiary = Color.White,
        background = Color.White,
        onBackground = Color(0xFF141414),
        surface = Color(0xFFF4F4F4),
        onSurface = Color(0xFF141414),
        onSurfaceVariant = Color(0xFF6B6B6B),
        surfaceVariant = Color(0xFFF0F0F0),
        surfaceContainer = Color(0xFFF0F0F0),
        surfaceContainerLow = Color(0xFFF4F4F4),
        surfaceContainerHigh = Color(0xFFF0F0F0),
        surfaceContainerHighest = Color(0xFFE2E2E2),
        surfaceTint = Color.Transparent,
        error = Color(0xFFC23A2B),
    )
    val type = Typography(
        headlineLarge = Type.largeTitle, headlineMedium = Type.title, headlineSmall = Type.title2,
        titleLarge = Type.title3.semibold, titleMedium = Type.headline, titleSmall = Type.subheadline.medium,
        bodyLarge = Type.body, bodyMedium = Type.subheadline, bodySmall = Type.footnote,
        labelLarge = Type.subheadline.medium, labelMedium = Type.caption.medium, labelSmall = Type.caption2.medium,
    )
    CompositionLocalProvider(LocalDark provides dark) {
        MaterialTheme(colorScheme = colors, typography = type) {
            CompositionLocalProvider(LocalTextStyle provides Type.body, LocalContentColor provides Palette.text, content = content)
        }
    }
}
