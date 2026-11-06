package com.codync.android

import android.content.Context
import android.os.Build
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.material3.Text
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import com.codync.android.design.*
import org.junit.After
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test

class BlockBUiTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Before fun isolatedEmulatorOnly() {
        assumeTrue(Build.PRODUCT.startsWith("sdk"))
        resetAppearancePrefs()
    }

    @After fun restoreAppearancePrefs() {
        resetAppearancePrefs()
    }

    private fun resetAppearancePrefs() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        context.getSharedPreferences("codync-appearance", Context.MODE_PRIVATE).edit().clear().commit()
        AppearancePreferences.get(context).update { AppearanceSettings() }
    }

    @Test fun threadHeaderAlignsIdentityLeftOpensInspectorAndNeverOverlapsActionsOnLongNames() {
        val store = ViewModelProvider(compose.activity)[AppStore::class.java]
        val computer = Computer("mac-studio", "Edwin's Mac Studio", "pub-key")
        val longName = "Architecture Reviewer & Security Auditor Bot With A Very Long Name"
        val bot = Bot(
            id = "long-bot",
            name = longName,
            avatarShape = "spark",
            avatarColor = "purple",
        )
        val state = AppState(
            loading = false,
            selectedBot = bot.id,
            bots = listOf(bot),
            computers = listOf(computer),
            connection = LinkState.Ready(HostRoute.Direct),
            actualConnection = LinkState.Ready(HostRoute.Direct),
            historyComplete = setOf(bot.id),
            entries = listOf(
                Entry("e1", 1, bot.id, rev = 1, kind = "user", data = kotlinx.serialization.json.buildJsonObject {
                    put("text", kotlinx.serialization.json.JsonPrimitive("Hello"))
                })
            ),
        )
        var inspectedBot: Bot? = null
        compose.runOnUiThread {
            compose.activity.setContent {
                Hosted { ChatScreen(state, store) { inspectedBot = it } }
            }
        }

        val backBounds = compose.onNodeWithContentDescription("Back").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        val identityBounds = compose.onNodeWithContentDescription("$longName details").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        val routinesBounds = compose.onNodeWithContentDescription("Open routines").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        val menuBounds = compose.onNode(hasContentDescription(" actions", substring = true)).assertIsDisplayed().fetchSemanticsNode().boundsInRoot

        // Identity stays left-aligned next to Back and never overlaps trailing actions
        assertTrue("Identity must be to the right of Back", identityBounds.left >= backBounds.right)
        assertTrue("Identity must not overlap Routines", identityBounds.right <= routinesBounds.left + 1f)
        assertTrue("Routines must be to the left of overflow menu", routinesBounds.right <= menuBounds.left + 1f)

        // Tapping the identity block invokes the bot inspector callback directly
        compose.onNodeWithContentDescription("$longName details").performClick()
        assertEquals(bot.id, inspectedBot?.id)
    }

    @Test fun appearanceSettingsPersistThemeAccentSurfaceDensityScaleAndEmitHaptics() {
        val store = ViewModelProvider(compose.activity)[AppStore::class.java]
        val recorder = RecordingCodyncHaptics(enabled = true)
        val computer = Computer("paired", "Fixture computer", "fixture-key")
        val state = AppState(
            loading = false,
            setupComplete = true,
            computers = listOf(computer),
            connection = LinkState.Ready(HostRoute.Direct),
        )
        var observedBg = Color.Unspecified
        var observedSurfaceStyle = SurfaceStyle.Glass
        var observedDensity = UiDensity.Standard
        var observedScale = ChatTextScale.DEFAULT

        compose.runOnUiThread {
            compose.activity.setContent {
                val prefs = rememberAppearancePreferences()
                CodyncTheme(preferences = prefs) {
                    OverlayRoot {
                        observedBg = Palette.background
                        observedSurfaceStyle = LocalSurfaceStyle.current
                        observedDensity = LocalUiDensity.current
                        observedScale = LocalChatTextScale.current
                        CompositionLocalProvider(LocalCodyncHaptics provides recorder) {
                            SettingsView(state, store)
                        }
                    }
                }
            }
        }

        // Select OLED Black theme mode via ChoicePicker
        compose.onNode(hasContentDescription("Theme:", substring = true)).performScrollTo().performClick()
        compose.onNodeWithText("OLED Black").performClick()
        compose.waitForIdle()
        assertEquals(Color(0xFF000000), observedBg)

        // Select Tonal surface style via ChoicePicker
        compose.onNode(hasContentDescription("Surface style:", substring = true)).performScrollTo().performClick()
        compose.onNodeWithText("Tonal (Lightweight)").performClick()
        compose.waitForIdle()
        assertEquals(SurfaceStyle.Tonal, observedSurfaceStyle)

        // Select Compact density via ChoicePicker
        compose.onNode(hasContentDescription("Density:", substring = true)).performScrollTo().performClick()
        compose.onNodeWithText("Compact").performClick()
        compose.waitForIdle()
        assertEquals(UiDensity.Compact, observedDensity)

        // Select 115% chat text scale via ChoicePicker
        compose.onNode(hasContentDescription("Chat text size:", substring = true)).performScrollTo().performClick()
        compose.onNodeWithText("115%").performClick()
        compose.waitForIdle()
        assertEquals(1.15f, observedScale, 0.001f)

        // Select Orange accent color via ChoicePicker
        compose.onNode(hasContentDescription("Accent color:", substring = true)).performScrollTo().performClick()
        compose.onNodeWithText("Orange").performClick()
        compose.waitForIdle()

        // Toggle Bot accent in chat switch to verify Toggle haptic
        compose.onNode(hasText("Bot accent in chat") and isToggleable()).performScrollTo().performClick()
        compose.waitForIdle()
        assertTrue(recorder.events.contains(CodyncHaptic.Toggle))

        // Verify persistence in SharedPreferences
        val reloaded = AppearancePreferences(
            compose.activity.applicationContext.getSharedPreferences("codync-appearance", Context.MODE_PRIVATE)
        ).settings
        assertEquals(ThemeMode.OledBlack, reloaded.themeMode)
        assertEquals(SurfaceStyle.Tonal, reloaded.surfaceStyle)
        assertEquals(UiDensity.Compact, reloaded.density)
        assertEquals(1.15f, reloaded.chatTextScale, 0.001f)
        assertEquals(AccentStyle.Orange, reloaded.accentStyle)
        assertFalse(reloaded.botAccent)
    }

    @Test fun botContextualAccentChangesPaletteBetweenBotsInChatScreen() {
        val greenBot = Bot("bot-green", "Scout", avatarColor = "green", avatarShape = "cloud")
        val purpleBot = Bot("bot-purple", "Nova", avatarColor = "purple", avatarShape = "spark")
        var activeBotId by mutableStateOf(greenBot.id)
        var chatAccent = Color.Unspecified

        compose.runOnUiThread {
            compose.activity.setContent {
                val prefs = remember {
                    AppearancePreferences.inMemory(
                        AppearanceSettings(
                            themeMode = ThemeMode.Dark,
                            accentStyle = AccentStyle.Default,
                            botAccent = true,
                        )
                    )
                }
                CodyncTheme(preferences = prefs) {
                    val active = if (activeBotId == greenBot.id) greenBot else purpleBot
                    BotAccentTheme(active.avatarColor) {
                        chatAccent = Palette.accent
                        Text(active.name)
                    }
                }
            }
        }

        compose.waitForIdle()
        val greenAccent = chatAccent
        compose.runOnUiThread { activeBotId = purpleBot.id }
        compose.waitForIdle()
        val purpleAccent = chatAccent
        assertNotEquals("Each bot's avatarColor must resolve a distinct contextual chat accent", greenAccent, purpleAccent)
    }

    @Test fun codyncSheetSupportsMediumDetentExpansionAndDismissal() {
        var open by mutableStateOf(true)
        compose.runOnUiThread {
            compose.activity.setContent {
                Hosted {
                    Box(Modifier.fillMaxSize())
                    CodyncSheet(
                        isPresented = open,
                        onDismiss = { open = false },
                        detents = setOf(SheetDetent.Medium, SheetDetent.Expanded),
                        initialDetent = SheetDetent.Medium,
                    ) {
                        ModalHeader("Sheet Detent Test")
                        Box(Modifier.height(240.dp)) {
                            Text("Medium sheet body")
                        }
                    }
                }
            }
        }

        compose.onNodeWithText("Sheet Detent Test").assertIsDisplayed()
        compose.onNodeWithText("Medium sheet body").assertIsDisplayed()
        saveSnapshot("block-b-sheet-medium.png")
        compose.onNodeWithContentDescription("Expand sheet").assertIsDisplayed().performClick()
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Collapse sheet").assertIsDisplayed()
        compose.onNodeWithContentDescription("Close").performClick()
        compose.waitForIdle()
        assertFalse(open)
    }

    @Test fun rendersVisualSnapshotsAndScrollsGlassVsTonalSurfaces() {
        val store = ViewModelProvider(compose.activity)[AppStore::class.java]
        val computer = Computer("mac-studio", "Edwin's Mac Studio", "pub-key")
        val bot = Bot("bot-nova", "Nova", avatarShape = "spark", avatarColor = "purple")
        val now = System.currentTimeMillis()
        val entries = (1..24).map { idx ->
            val fromUser = idx % 2 == 1
            Entry(
                id = "msg-$idx",
                seq = idx.toLong(),
                botId = bot.id,
                rev = 1,
                kind = if (fromUser) "user" else "agent",
                data = kotlinx.serialization.json.buildJsonObject {
                    put(
                        "text",
                        kotlinx.serialization.json.JsonPrimitive(
                            if (fromUser) "Verify Bloque B appearance item #$idx with contextual accent."
                            else "Confirmed item #$idx: contrast >= 4.5:1, **Markdown link** https://codync.dev and `tonal` surface ready."
                        ),
                    )
                    if (!fromUser) {
                        put("author", kotlinx.serialization.json.JsonPrimitive(bot.id))
                        put("final", kotlinx.serialization.json.JsonPrimitive(true))
                    }
                },
                createdAt = now + idx * 1000L,
            )
        }
        val state = AppState(
            loading = false,
            selectedBot = bot.id,
            bots = listOf(bot),
            computers = listOf(computer),
            connection = LinkState.Ready(HostRoute.Direct),
            actualConnection = LinkState.Ready(HostRoute.Direct),
            historyComplete = setOf(bot.id),
            entries = entries,
        )
        val prefs = AppearancePreferences.inMemory(
            AppearanceSettings(
                themeMode = ThemeMode.OledBlack,
                accentStyle = AccentStyle.Violet,
                botAccent = true,
                surfaceStyle = SurfaceStyle.Tonal,
                chatTextScale = 1.15f,
                density = UiDensity.Compact,
            )
        )
        var showSettings by mutableStateOf(false)

        compose.runOnUiThread {
            compose.activity.setContent {
                CodyncTheme(preferences = prefs) {
                    OverlayRoot {
                        if (showSettings) {
                            SettingsView(state, store)
                        } else {
                            ChatScreen(state, store, {})
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
        saveSnapshot("block-b-chat-oled-tonal.png")

        // Switch to Light + Liquid Glass + Green bot accent and capture snapshot
        compose.runOnUiThread {
            prefs.update {
                it.copy(
                    themeMode = ThemeMode.Light,
                    accentStyle = AccentStyle.Green,
                    surfaceStyle = SurfaceStyle.Glass,
                    chatTextScale = 1.0f,
                    density = UiDensity.Standard,
                )
            }
        }
        compose.waitForIdle()
        saveSnapshot("block-b-chat-light-glass.png")

        // Render Settings Appearance section in Dark mode and capture snapshot
        compose.runOnUiThread {
            prefs.setThemeMode(ThemeMode.Dark)
            showSettings = true
        }
        compose.waitForIdle()
        compose.onNode(hasContentDescription("Theme:", substring = true)).performScrollTo()
        compose.waitForIdle()
        saveSnapshot("block-b-settings-appearance.png")
    }

    private fun saveSnapshot(fileName: String) {
        val bitmap = compose.onRoot().captureToImage()
        val androidBitmap = bitmap.asAndroidBitmap()
        java.io.File(compose.activity.cacheDir, fileName).outputStream().use {
            androidBitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)
        }
    }
}
