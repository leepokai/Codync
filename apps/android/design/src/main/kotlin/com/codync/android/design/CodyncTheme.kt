package com.codync.android.design

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.core.view.WindowCompat

/** Keep these pairs aligned with CodyncKit/Design/Theme.swift; obeys manual theme via `Palette`. */
val botMessageBackground: Color
    @Composable @ReadOnlyComposable get() = Palette.botMessageBackground

private tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}

private fun ResolvedPalette.toMaterialColorScheme(): ColorScheme = if (isDark) darkColorScheme(
    primary = accentFill,
    onPrimary = onAccent,
    secondary = accent,
    onSecondary = onAccent,
    tertiary = accent,
    onTertiary = onAccent,
    background = background,
    onBackground = text,
    surface = surface,
    onSurface = text,
    onSurfaceVariant = secondary,
    surfaceVariant = bubbleAgent,
    surfaceContainer = bubbleAgent,
    surfaceContainerLow = surface,
    surfaceContainerHigh = bubbleAgent,
    surfaceContainerHighest = bubbleUser,
    surfaceTint = Color.Transparent,
    error = danger,
) else lightColorScheme(
    primary = accentFill,
    onPrimary = onAccent,
    secondary = accent,
    onSecondary = onAccent,
    tertiary = accent,
    onTertiary = onAccent,
    background = background,
    onBackground = text,
    surface = surface,
    onSurface = text,
    onSurfaceVariant = secondary,
    surfaceVariant = bubbleAgent,
    surfaceContainer = bubbleAgent,
    surfaceContainerLow = surface,
    surfaceContainerHigh = bubbleAgent,
    surfaceContainerHighest = bubbleUser,
    surfaceTint = Color.Transparent,
    error = danger,
)

@Composable
private fun dynamicSchemeFor(accentStyle: AccentStyle, dark: Boolean): ColorScheme? {
    if (accentStyle != AccentStyle.Dynamic || Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return null
    val context = LocalContext.current
    return if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
}

@Composable
fun CodyncTheme(
    preferences: AppearancePreferences = rememberAppearancePreferences(),
    settings: AppearanceSettings = preferences.settings,
    botAvatarColor: String? = null,
    content: @Composable () -> Unit,
) {
    val systemDark = isSystemInDarkTheme()
    val effectiveDark = settings.themeMode.resolvesDark(systemDark)
    val dynamicScheme = dynamicSchemeFor(settings.accentStyle, effectiveDark)

    val palette = resolvePalette(
        systemDark = systemDark,
        settings = settings,
        dynamicScheme = dynamicScheme,
        botAvatarColor = botAvatarColor,
    )
    val colors = palette.toMaterialColorScheme()
    val type = Typography(
        headlineLarge = Type.largeTitle, headlineMedium = Type.title, headlineSmall = Type.title2,
        titleLarge = Type.title3.semibold, titleMedium = Type.headline, titleSmall = Type.subheadline.medium,
        bodyLarge = Type.body, bodyMedium = Type.subheadline, bodySmall = Type.footnote,
        labelLarge = Type.subheadline.medium, labelMedium = Type.caption.medium, labelSmall = Type.caption2.medium,
    )
    val haptics = rememberCodyncHaptics(settings.hapticsEnabled)
    val view = LocalView.current
    if (!view.isInEditMode) {
        SideEffect {
            val window = view.context.findActivity()?.window ?: return@SideEffect
            val controller = WindowCompat.getInsetsController(window, view)
            controller.isAppearanceLightStatusBars = !palette.isDark
            controller.isAppearanceLightNavigationBars = !palette.isDark
        }
    }
    CompositionLocalProvider(
        LocalAppearancePreferences provides preferences,
        LocalAppearance provides settings,
        LocalDark provides palette.isDark,
        LocalPalette provides palette,
        LocalSurfaceStyle provides settings.surfaceStyle,
        LocalUiDensity provides settings.density,
        LocalChatTextScale provides settings.chatTextScale,
        LocalCodyncHaptics provides haptics,
    ) {
        MaterialTheme(colorScheme = colors, typography = type) {
            CompositionLocalProvider(
                LocalTextStyle provides Type.body,
                LocalContentColor provides Palette.text,
                content = content,
            )
        }
    }
}

/**
 * Applies a contextual bot accent derived from [avatarColor] inside a chat or thread
 * without mutating global preferences or semantic error/warning/permission colors.
 */
@Composable
fun BotAccentTheme(avatarColor: String?, content: @Composable () -> Unit) {
    val settings = LocalAppearance.current
    if (!settings.botAccent || avatarColor.isNullOrBlank()) {
        content()
        return
    }
    val systemDark = LocalDark.current
    val dynamicScheme = dynamicSchemeFor(settings.accentStyle, systemDark)
    val palette = resolvePalette(
        systemDark = systemDark,
        settings = settings,
        dynamicScheme = dynamicScheme,
        botAvatarColor = avatarColor,
    )
    CompositionLocalProvider(LocalPalette provides palette) {
        MaterialTheme(colorScheme = palette.toMaterialColorScheme(), content = content)
    }
}
