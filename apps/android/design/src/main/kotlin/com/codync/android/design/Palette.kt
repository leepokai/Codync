package com.codync.android.design

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

/** Whether the dark half of each color pair applies; `CodyncTheme` sets it from the system. */
val LocalDark = staticCompositionLocalOf { false }

private fun hex(value: Long) = Color(0xFF000000 or value)

@Composable @ReadOnlyComposable
private fun pair(light: Long, dark: Long) = hex(if (LocalDark.current) dark else light)

/**
 * Black and white: a pure black (or white) ground, grey bubbles, and color that comes only
 * from the bots; amber marks "needs you" and red marks errors. Mirrors CodyncKit `Palette`.
 */
object Palette {
    val background: Color @Composable @ReadOnlyComposable get() = pair(0xFFFFFF, 0x0A0A0A)
    val surface: Color @Composable @ReadOnlyComposable get() = pair(0xF4F4F4, 0x141414)
    val bubbleAgent: Color @Composable @ReadOnlyComposable get() = pair(0xF0F0F0, 0x1C1C1C)
    val bubbleUser: Color @Composable @ReadOnlyComposable get() = pair(0xE2E2E2, 0x363636)
    val border: Color @Composable @ReadOnlyComposable get() = pair(0xE6E6E6, 0x262626)
    val text: Color @Composable @ReadOnlyComposable get() = pair(0x141414, 0xF2F2F2)
    val secondary: Color @Composable @ReadOnlyComposable get() = pair(0x6B6B6B, 0x9A9A9A)
    val tertiary: Color @Composable @ReadOnlyComposable get() = pair(0x767676, 0x929292)
    /** Ink used for fills (primary buttons, the send button, unread badges). */
    val accentFill: Color @Composable @ReadOnlyComposable get() = pair(0x000000, 0xFFFFFF)
    /** Ink readable as text and tint on the background. */
    val accent: Color @Composable @ReadOnlyComposable get() = pair(0x000000, 0xFFFFFF)
    val onAccent: Color @Composable @ReadOnlyComposable get() = pair(0xFFFFFF, 0x000000)
    val accentDim: Color @Composable @ReadOnlyComposable get() = pair(0xD9D9D9, 0x333333)
    val danger: Color @Composable @ReadOnlyComposable get() = pair(0xC23A2B, 0xF0A7A7)
    val warning: Color get() = hex(0xF0A030)
    /** A bot that needs you, in widgets and task cards: readable amber in both modes. */
    val attention: Color @Composable @ReadOnlyComposable get() = pair(0x936000, 0xECAF52)
    val codeBackground: Color @Composable @ReadOnlyComposable get() = pair(0xF4F4F4, 0x111111)
    val added: Color @Composable @ReadOnlyComposable get() = pair(0x2E7D32, 0x8FD18B)
    val removed: Color @Composable @ReadOnlyComposable get() = pair(0xC62828, 0xF0A7A7)
}
