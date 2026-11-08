package com.codync.android.design

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import androidx.compose.material3.ColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.LocalContext
import kotlin.math.max
import kotlin.math.min

/** Manual or system-following theme mode. */
enum class ThemeMode(val id: String, val label: String) {
    System("system", "System"),
    Light("light", "Light"),
    Dark("dark", "Dark"),
    OledBlack("oled", "OLED Black");

    fun resolvesDark(systemDark: Boolean): Boolean = when (this) {
        System -> systemDark
        Light -> false
        Dark, OledBlack -> true
    }

    companion object {
        fun fromId(id: String?): ThemeMode = entries.firstOrNull { it.id == id } ?: System
    }
}

/** Global accent palette: Codync monochrome, Android 12+ Material You dynamic color, or curated accents. */
enum class AccentStyle(val id: String, val label: String) {
    Default("default", "Monochrome"),
    Dynamic("dynamic", "Dynamic (Material You)"),
    Blue("blue", "Blue"),
    Green("green", "Green"),
    Orange("orange", "Orange"),
    Violet("violet", "Violet"),
    Magenta("magenta", "Magenta");

    companion object {
        fun fromId(id: String?): AccentStyle = entries.firstOrNull { it.id == id } ?: Default
        fun available(sdkInt: Int = Build.VERSION.SDK_INT): List<AccentStyle> =
            if (sdkInt >= Build.VERSION_CODES.S) entries else entries.filter { it != Dynamic }
    }
}

/** Material surface rendering mode: refracted Liquid Glass or lightweight tonal surfaces. */
enum class SurfaceStyle(val id: String, val label: String) {
    Glass("glass", "Liquid Glass"),
    Tonal("tonal", "Tonal (Lightweight)");

    companion object {
        fun fromId(id: String?): SurfaceStyle = entries.firstOrNull { it.id == id } ?: Glass
    }
}

/** Row and container spacing density; touch targets remain >= 48.dp in both modes. */
enum class UiDensity(val id: String, val label: String) {
    Standard("standard", "Standard"),
    Compact("compact", "Compact");

    companion object {
        fun fromId(id: String?): UiDensity = entries.firstOrNull { it.id == id } ?: Standard
    }
}

/** How a bot's character is drawn: plush (shaded dots under a light) or flat (even dots, no light). */
enum class AvatarStyle(val id: String, val label: String) {
    Plush("plush", "Plush"),
    Flat("flat", "Flat");

    companion object {
        fun fromId(id: String?): AvatarStyle = entries.firstOrNull { it.id == id } ?: Plush
    }
}

/** Discrete chat text scale options aligned with the desktop app's `TEXT_SIZES` semantics. */
object ChatTextScale {
    const val MIN = 0.85f
    const val MAX = 1.30f
    const val DEFAULT = 1.0f

    val options: List<Pair<Float, String>> = listOf(
        0.85f to "85%",
        0.92f to "92%",
        1.00f to "100% (Default)",
        1.08f to "108%",
        1.15f to "115%",
        1.25f to "125%",
        1.30f to "130%",
    )

    fun clamp(scale: Float): Float = if (scale.isFinite()) scale.coerceIn(MIN, MAX) else DEFAULT

    /** Caps combined (system fontScale * local chatScale) so huge accessibility settings never clip controls. */
    fun effectiveMultiplier(systemFontScale: Float, chatScale: Float): Float {
        val safeSystem = if (systemFontScale.isFinite() && systemFontScale > 0f) systemFontScale else 1f
        val clampedChat = clamp(chatScale)
        val maxCombined = max(safeSystem, 2.0f)
        return (safeSystem * clampedChat).coerceAtMost(maxCombined) / safeSystem
    }
}

/** Device-scoped appearance configuration. */
data class AppearanceSettings(
    val themeMode: ThemeMode = ThemeMode.System,
    val accentStyle: AccentStyle = AccentStyle.Default,
    val botAccent: Boolean = true,
    val surfaceStyle: SurfaceStyle = SurfaceStyle.Glass,
    val chatTextScale: Float = ChatTextScale.DEFAULT,
    val density: UiDensity = UiDensity.Standard,
    val hapticsEnabled: Boolean = true,
    val avatarStyle: AvatarStyle = AvatarStyle.Plush,
)

/** Observable device-scoped appearance preferences, persisted in SharedPreferences. */
class AppearancePreferences(private val prefs: SharedPreferences?) {
    var settings by mutableStateOf(load())
        private set

    private fun load(): AppearanceSettings {
        val p = prefs ?: return AppearanceSettings()
        return AppearanceSettings(
            themeMode = ThemeMode.fromId(p.getString("themeMode", ThemeMode.System.id)),
            accentStyle = AccentStyle.fromId(p.getString("accentStyle", AccentStyle.Default.id)),
            botAccent = p.getBoolean("botAccent", true),
            surfaceStyle = SurfaceStyle.fromId(p.getString("surfaceStyle", SurfaceStyle.Glass.id)),
            chatTextScale = ChatTextScale.clamp(p.getFloat("chatTextScale", ChatTextScale.DEFAULT)),
            density = UiDensity.fromId(p.getString("density", UiDensity.Standard.id)),
            hapticsEnabled = p.getBoolean("hapticsEnabled", true),
            avatarStyle = AvatarStyle.fromId(p.getString("avatarStyle", AvatarStyle.Plush.id)),
        )
    }

    fun update(transform: (AppearanceSettings) -> AppearanceSettings) {
        val next = transform(settings).let { it.copy(chatTextScale = ChatTextScale.clamp(it.chatTextScale)) }
        settings = next
        prefs?.edit()
            ?.putString("themeMode", next.themeMode.id)
            ?.putString("accentStyle", next.accentStyle.id)
            ?.putBoolean("botAccent", next.botAccent)
            ?.putString("surfaceStyle", next.surfaceStyle.id)
            ?.putFloat("chatTextScale", next.chatTextScale)
            ?.putString("density", next.density.id)
            ?.putBoolean("hapticsEnabled", next.hapticsEnabled)
            ?.putString("avatarStyle", next.avatarStyle.id)
            ?.apply()
    }

    fun setThemeMode(mode: ThemeMode) = update { it.copy(themeMode = mode) }
    fun setAccentStyle(style: AccentStyle) = update { it.copy(accentStyle = style) }
    fun setBotAccent(enabled: Boolean) = update { it.copy(botAccent = enabled) }
    fun setSurfaceStyle(style: SurfaceStyle) = update { it.copy(surfaceStyle = style) }
    fun setChatTextScale(scale: Float) = update { it.copy(chatTextScale = scale) }
    fun setDensity(density: UiDensity) = update { it.copy(density = density) }
    fun setHapticsEnabled(enabled: Boolean) = update { it.copy(hapticsEnabled = enabled) }
    fun setAvatarStyle(style: AvatarStyle) = update { it.copy(avatarStyle = style) }

    companion object {
        @Volatile private var instance: AppearancePreferences? = null

        fun get(context: Context): AppearancePreferences = instance ?: synchronized(this) {
            instance ?: AppearancePreferences(
                context.applicationContext.getSharedPreferences("codync-appearance", Context.MODE_PRIVATE)
            ).also { instance = it }
        }

        fun inMemory(initial: AppearanceSettings = AppearanceSettings()): AppearancePreferences =
            AppearancePreferences(null).also { it.update { initial } }
    }
}

@Composable fun rememberAppearancePreferences(): AppearancePreferences {
    val context = LocalContext.current
    return remember(context) { AppearancePreferences.get(context) }
}

/** Semantic color tokens resolved for the active theme, accent, and optional bot context. */
data class ResolvedPalette(
    val isDark: Boolean,
    val isOled: Boolean,
    val background: Color,
    val surface: Color,
    val bubbleAgent: Color,
    val bubbleUser: Color,
    val border: Color,
    val text: Color,
    val secondary: Color,
    val tertiary: Color,
    val accentFill: Color,
    val accent: Color,
    val onAccent: Color,
    val accentDim: Color,
    val danger: Color,
    val warning: Color,
    val attention: Color,
    val codeBackground: Color,
    val added: Color,
    val removed: Color,
    val botMessageBackground: Color,
)

/** WCAG 2.1 relative luminance contrast ratio (1.0 .. 21.0). */
fun contrastRatio(first: Color, second: Color): Float {
    val l1 = first.luminance()
    val l2 = second.luminance()
    val lighter = max(l1, l2)
    val darker = min(l1, l2)
    return (lighter + 0.05f) / (darker + 0.05f)
}

/** Picks black or white ink with the higher contrast against [fill]. */
fun readableOn(fill: Color): Color =
    if (contrastRatio(Color.White, fill) >= contrastRatio(Color(0xFF141414), fill)) Color.White else Color(0xFF141414)

/** Adjusts [candidate] toward white (in dark mode) or black (in light mode) until contrast with [ground] >= [minRatio]. */
fun ensureReadableAccent(candidate: Color, ground: Color, dark: Boolean, minRatio: Float = 4.5f): Color {
    if (contrastRatio(candidate, ground) >= minRatio) return candidate
    val target = if (dark) Color.White else Color.Black
    for (step in 1..20) {
        val mixed = lerp(candidate, target, step / 20f)
        if (contrastRatio(mixed, ground) >= minRatio) return mixed
    }
    return target
}

private fun curatedAccentColor(style: AccentStyle): Color? = when (style) {
    AccentStyle.Default, AccentStyle.Dynamic -> null
    AccentStyle.Blue -> AvatarPalette.tint("blue")
    AccentStyle.Green -> AvatarPalette.tint("green")
    AccentStyle.Orange -> AvatarPalette.tint("orange")
    AccentStyle.Violet -> AvatarPalette.tint("violet")
    AccentStyle.Magenta -> AvatarPalette.tint("magenta")
}

/**
 * Central appearance resolver:
 * - Separates ground/surfaces (Light, Dark, OLED Black), global/dynamic accent, and bot chat accent.
 * - Semantic colors (`danger`, `warning`, `attention`, `added`, `removed`) remain stable and legible.
 */
fun resolvePalette(
    systemDark: Boolean,
    settings: AppearanceSettings = AppearanceSettings(),
    dynamicScheme: ColorScheme? = null,
    botAvatarColor: String? = null,
): ResolvedPalette {
    val dark = settings.themeMode.resolvesDark(systemDark)
    val oled = settings.themeMode == ThemeMode.OledBlack
    val dynamic = if (settings.accentStyle == AccentStyle.Dynamic) dynamicScheme else null

    val baseBackground = when {
        oled -> Color(0xFF000000)
        dynamic != null -> if (dark) lerp(Color(0xFF0A0A0A), dynamic.background, 0.35f) else lerp(Color.White, dynamic.background, 0.35f)
        dark -> Color(0xFF0A0A0A)
        else -> Color(0xFFFFFFFF)
    }
    val baseSurface = when {
        oled -> Color(0xFF0A0A0A)
        dynamic != null -> if (dark) lerp(Color(0xFF141414), dynamic.surfaceContainerLow, 0.45f) else lerp(Color(0xFFF4F4F4), dynamic.surfaceContainerLow, 0.45f)
        dark -> Color(0xFF141414)
        else -> Color(0xFFF4F4F4)
    }
    val baseBubbleAgent = when {
        oled -> Color(0xFF121212)
        dynamic != null -> if (dark) lerp(Color(0xFF1C1C1C), dynamic.surfaceContainer, 0.45f) else lerp(Color(0xFFF0F0F0), dynamic.surfaceContainer, 0.45f)
        dark -> Color(0xFF1C1C1C)
        else -> Color(0xFFF0F0F0)
    }
    val baseBubbleUser = when {
        oled -> Color(0xFF262626)
        dynamic != null -> if (dark) lerp(Color(0xFF363636), dynamic.surfaceContainerHighest, 0.45f) else lerp(Color(0xFFE2E2E2), dynamic.surfaceContainerHighest, 0.45f)
        dark -> Color(0xFF363636)
        else -> Color(0xFFE2E2E2)
    }
    val border = if (oled) Color(0xFF1F1F1F) else if (dark) Color(0xFF262626) else Color(0xFFE6E6E6)
    val text = if (dark) Color(0xFFF2F2F2) else Color(0xFF141414)
    val secondary = if (dark) Color(0xFF9A9A9A) else Color(0xFF6B6B6B)
    val tertiary = if (dark) Color(0xFF929292) else Color(0xFF767676)

    val rawGlobalAccent: Color? = dynamic?.primary ?: curatedAccentColor(settings.accentStyle)
    // Contextual bot accent overrides chat accent tokens when enabled and a bot color is active.
    val rawBotAccent: Color? = if (settings.botAccent && !botAvatarColor.isNullOrBlank()) {
        AvatarPalette.tint(botAvatarColor)
    } else null

    val chosenSeed = rawBotAccent ?: rawGlobalAccent
    val accentFill: Color
    val accent: Color
    val onAccent: Color
    if (chosenSeed == null) {
        accentFill = if (dark) Color.White else Color.Black
        accent = if (dark) Color.White else Color.Black
        onAccent = if (dark) Color.Black else Color.White
    } else {
        accent = ensureReadableAccent(chosenSeed, baseBackground, dark, minRatio = 4.5f)
        // Fill keeps vibrant color while guaranteeing >= 4.5:1 contrast with onAccent.
        val candidateFill = if (dark) lerp(chosenSeed, Color.White, 0.12f) else lerp(chosenSeed, Color.Black, 0.08f)
        accentFill = if (contrastRatio(Color.White, candidateFill) >= 4.5f || contrastRatio(Color(0xFF141414), candidateFill) >= 4.5f) {
            candidateFill
        } else {
            ensureReadableAccent(candidateFill, baseBackground, dark, minRatio = 4.5f)
        }
        onAccent = readableOn(accentFill)
    }

    val bubbleUser = if (rawBotAccent != null) {
        val tinted = lerp(baseBubbleUser, rawBotAccent, if (dark) 0.22f else 0.14f)
        if (contrastRatio(text, tinted) >= 4.5f) tinted else baseBubbleUser
    } else baseBubbleUser

    return ResolvedPalette(
        isDark = dark,
        isOled = oled,
        background = baseBackground,
        surface = baseSurface,
        bubbleAgent = baseBubbleAgent,
        bubbleUser = bubbleUser,
        border = border,
        text = text,
        secondary = secondary,
        tertiary = tertiary,
        accentFill = accentFill,
        accent = accent,
        onAccent = onAccent,
        accentDim = if (dark) Color(0xFF333333) else Color(0xFFD9D9D9),
        danger = if (dark) Color(0xFFF0A7A7) else Color(0xFFC23A2B),
        warning = Color(0xFFF0A030),
        attention = if (dark) Color(0xFFECAF52) else Color(0xFF936000),
        codeBackground = if (oled) Color(0xFF080808) else if (dark) Color(0xFF111111) else Color(0xFFF4F4F4),
        added = if (dark) Color(0xFF8FD18B) else Color(0xFF2E7D32),
        removed = if (dark) Color(0xFFF0A7A7) else Color(0xFFC62828),
        botMessageBackground = if (dark) Color(0xFF15263E) else Color(0xFFEAF2FF),
    )
}

val LocalAppearancePreferences = staticCompositionLocalOf<AppearancePreferences?> { null }
val LocalAppearance = staticCompositionLocalOf { AppearanceSettings() }
val LocalSurfaceStyle = staticCompositionLocalOf { SurfaceStyle.Glass }
val LocalUiDensity = staticCompositionLocalOf { UiDensity.Standard }
val LocalChatTextScale = staticCompositionLocalOf { ChatTextScale.DEFAULT }
val LocalAvatarStyle = staticCompositionLocalOf { AvatarStyle.Plush }
val LocalPalette = staticCompositionLocalOf { resolvePalette(systemDark = false) }
