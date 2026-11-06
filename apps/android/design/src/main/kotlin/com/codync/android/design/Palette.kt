package com.codync.android.design

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

/** Whether the dark half of each color pair applies; `CodyncTheme` sets it from the resolved theme mode. */
val LocalDark = staticCompositionLocalOf { false }

private val fallbackLightPalette = resolvePalette(systemDark = false)
private val fallbackDarkPalette = resolvePalette(systemDark = true)

@Composable @ReadOnlyComposable
private fun currentPalette(): ResolvedPalette {
    val palette = LocalPalette.current
    val dark = LocalDark.current
    // Subtrees that override LocalDark directly (e.g. ScreenView or dark widget previews) get dark tokens.
    return if (palette.isDark == dark) palette else if (dark) fallbackDarkPalette else fallbackLightPalette
}

/**
 * Semantic color tokens resolved by `CodyncTheme` (Light, Dark, OLED Black, Dynamic color,
 * curated accents, and contextual bot chat tinting). Mirrors CodyncKit `Palette`.
 */
object Palette {
    val background: Color @Composable @ReadOnlyComposable get() = currentPalette().background
    val surface: Color @Composable @ReadOnlyComposable get() = currentPalette().surface
    val bubbleAgent: Color @Composable @ReadOnlyComposable get() = currentPalette().bubbleAgent
    val bubbleUser: Color @Composable @ReadOnlyComposable get() = currentPalette().bubbleUser
    val border: Color @Composable @ReadOnlyComposable get() = currentPalette().border
    val text: Color @Composable @ReadOnlyComposable get() = currentPalette().text
    val secondary: Color @Composable @ReadOnlyComposable get() = currentPalette().secondary
    val tertiary: Color @Composable @ReadOnlyComposable get() = currentPalette().tertiary
    /** Ink used for fills (primary buttons, the send button, unread badges). */
    val accentFill: Color @Composable @ReadOnlyComposable get() = currentPalette().accentFill
    /** Ink readable as text and tint on the background. */
    val accent: Color @Composable @ReadOnlyComposable get() = currentPalette().accent
    val onAccent: Color @Composable @ReadOnlyComposable get() = currentPalette().onAccent
    val accentDim: Color @Composable @ReadOnlyComposable get() = currentPalette().accentDim
    val danger: Color @Composable @ReadOnlyComposable get() = currentPalette().danger
    val warning: Color @Composable @ReadOnlyComposable get() = currentPalette().warning
    /** A bot that needs you, in widgets and task cards: readable amber in both modes. */
    val attention: Color @Composable @ReadOnlyComposable get() = currentPalette().attention
    val codeBackground: Color @Composable @ReadOnlyComposable get() = currentPalette().codeBackground
    val added: Color @Composable @ReadOnlyComposable get() = currentPalette().added
    val removed: Color @Composable @ReadOnlyComposable get() = currentPalette().removed
    val botMessageBackground: Color @Composable @ReadOnlyComposable get() = currentPalette().botMessageBackground
}
