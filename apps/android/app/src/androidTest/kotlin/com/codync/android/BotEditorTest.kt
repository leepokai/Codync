package com.codync.android

import android.graphics.Bitmap
import android.os.Build
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import com.codync.android.design.AvatarPalette
import com.codync.android.design.LocalDismissModal
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import java.io.File

/** Read-only UI fixtures for the bot settings and the New bot / New group chat dialogs: no test taps Save or Create, so production bots are never changed. */
class BotEditorTest {
    @get:Rule val compose = createComposeRule()
    private val store = previewStore(Computer("fixture", "Fixture computer", "fixture-key"))
    private val scout = BotDraft(id = "fixture", name = "Scout", avatarShape = "blob", avatarColor = "blue")

    @Before fun isolatedEmulatorOnly() { assumeTrue(Build.PRODUCT.startsWith("sdk")) }

    @Test fun settingsShowTheAvatarFirstAndTheAgentPickerOffersACustomCommand() {
        var closed = false
        render(scout, close = { closed = true })
        compose.onNodeWithText("Settings").assertIsDisplayed()
        compose.onNodeWithContentDescription("Save").assertIsEnabled()
        // The Avatar row unfolds the picker.
        compose.onNodeWithContentDescription("Edit Bot avatar").performClick()
        compose.onNodeWithContentDescription("blob shape").assertIsSelected()
        compose.onNodeWithContentDescription("cloud shape").performClick().assertIsSelected()
        compose.onNodeWithContentDescription("blob shape").assertIsNotSelected()
        compose.onNodeWithContentDescription("Green").performScrollTo().performClick().assertIsSelected()
        val name = compose.onNode(hasSetTextAction() and hasText("Scout")).assertIsDisplayed()
        snapshot("bot-editor-settings-preview.png")
        name.performTextReplacement("")
        compose.onNodeWithContentDescription("Save").assertIsNotEnabled()
        // The agent row ("Claude · Default") unfolds into the model list; the agent's name opens the agents.
        compose.onNodeWithText("Claude · Default").performScrollTo().performClick()
        compose.onNodeWithText("Claude").performScrollTo().performClick()
        // The agent list scrolls on its own; bring the picker's last line, under it, into the form's view.
        compose.onNodeWithText("Changing the model starts a new agent session. Chat history is kept.").performScrollTo()
        compose.onNodeWithText("Codex (not installed)").assertIsDisplayed()
        compose.onNodeWithText("Custom command").performClick()
        compose.onNode(hasSetTextAction() and hasText("ACP command", substring = true)).performScrollTo().performTextInput("my-agent --acp")
        compose.onNodeWithText("my-agent --acp").assertIsDisplayed()
        compose.onNodeWithText("Custom command").performScrollTo().performClick()
        compose.onNodeWithText("Changing the model starts a new agent session. Chat history is kept.").performScrollTo()
        compose.onNodeWithText("Codex (not installed)").performClick()
        compose.onNodeWithText("Install Codex on your computer.").performScrollTo().assertIsDisplayed()
        compose.onNode(hasSetTextAction() and hasText("ACP command", substring = true)).assertDoesNotExist()
        compose.onNodeWithContentDescription("Close").performClick()
        assertTrue(closed)
    }

    @Test fun aNewBotIsItsFaceAndAnOptionalName() {
        compose.setContent { Hosted { NewBotView(fixture(), store) } }
        compose.onNodeWithText("New bot").assertIsDisplayed()
        compose.onNode(hasSetTextAction() and hasText("Name your bot")).assertIsDisplayed().assertIsFocused()
        compose.onNodeWithContentDescription("Create").assertIsEnabled()
        compose.onNodeWithContentDescription("Agent: Claude").assertDoesNotExist()
        compose.onNodeWithContentDescription("teardrop shape").performClick().assertIsSelected()
        compose.onNodeWithContentDescription("Gray").performClick().assertIsSelected()
        // Every shape and color shows at once: the grids wrap instead of scrolling sideways.
        (AvatarPalette.shapes.map { "$it shape" } + AvatarPalette.colors.keys.map { it.replaceFirstChar(Char::uppercase) })
            .forEach { compose.onNodeWithContentDescription(it).assertIsDisplayed() }
        snapshot("new-bot-preview.png")
    }

    @Test fun anOfflineNewBotCannotBeCreated() {
        compose.setContent { Hosted { NewBotView(fixture().copy(actualConnection = LinkState.Connecting), store) } }
        compose.onNodeWithContentDescription("Create").assertIsNotEnabled()
        compose.onNodeWithText("Connect this computer to create the bot.").assertIsDisplayed()
    }

    @Test fun aNewGroupIsNamedAfterThePickedBotsUntilNamed() {
        val bots = listOf(Bot("a", "Scout", avatarShape = "blob"), Bot("b", "Reviewer", avatarShape = "hex"))
        compose.setContent { Hosted { NewGroupView(fixture().copy(bots = bots), store) } }
        compose.onNodeWithText("New group chat").assertIsDisplayed()
        compose.onNodeWithContentDescription("Create").assertIsNotEnabled()
        compose.onNode(hasSetTextAction() and hasText("Name your group")).assertIsDisplayed()
        compose.onNodeWithContentDescription("Scout").performClick().assertIsSelected()
        compose.onNodeWithContentDescription("Reviewer").performClick().assertIsSelected()
        compose.onNode(hasSetTextAction() and hasText("Scout, Reviewer")).assertIsDisplayed()
        compose.onNodeWithContentDescription("Create").assertIsEnabled()
        compose.onNodeWithContentDescription("Scout").performClick().assertIsNotSelected()
        snapshot("new-group-preview.png")
    }

    @Test fun aNewGroupWithoutBotsSaysToCreateOneFirst() {
        compose.setContent { Hosted { NewGroupView(fixture(), store) } }
        compose.onNodeWithText("No bots yet. Create a bot first.").assertIsDisplayed()
        compose.onNodeWithContentDescription("Create").assertIsNotEnabled()
    }

    @Test fun workspaceAndPermissionsUseMenusWhileNotificationsAndPluginsUseRealSwitches() {
        val plugins = PluginListing(listOf(InstalledConnector("git", "GitHub", kind = "local", command = "git-server")),
            listOf(InstalledSkill("review", "Review changes", "Review before pushing", "local", "/skills/review.md")))
        render(scout.copy(cwd = "/home/Project", notify = null), fixture().copy(plugins = plugins))
        compose.onNodeWithText("This project is the default starting folder. The bot can work elsewhere when you ask.").performScrollTo().assertIsDisplayed()
        compose.onNodeWithContentDescription("Workspace").performClick()
        compose.onNodeWithText("Personal workspace").performClick()
        compose.onNodeWithText("This bot has its own space for files. It can work in other folders when you ask.").assertIsDisplayed()
        compose.onNodeWithContentDescription("Permissions: Approve automatically").performScrollTo().performClick()
        compose.onNodeWithText("Ask me").performClick()
        compose.onNodeWithText("You'll get an approval card and a notification whenever the agent asks.").assertIsDisplayed()
        toggle("Notifications").performScrollTo().assertIsOn().performClick().assertIsOff()
        toggle("Use the computer").performScrollTo().assertIsOff()
        // Connectors start off until picked, as on the iPhone.
        toggle("GitHub").performScrollTo().assertIsOff().performClick().assertIsOn()
        toggle("Review changes").performScrollTo().assertIsOff().performClick().assertIsOn()
        compose.onNodeWithContentDescription("Workspace").performScrollTo().performClick()
        compose.onNodeWithText("Choose project folder…").performClick()
        compose.onNodeWithText("Choose workspace").assertIsDisplayed()
    }

    @Test fun offlineSettingsDisableSavingButKeepTheModelEditable() {
        render(scout.copy(model = "old-model"), fixture().copy(actualConnection = LinkState.Connecting))
        compose.onNodeWithContentDescription("Save").assertIsNotEnabled()
        compose.onNodeWithText("Reconnect to this computer to save settings.").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Claude · old-model").performScrollTo().performClick()
        compose.onNodeWithText("Connect to this computer to load its models. You can still enter a model ID.").performScrollTo().assertIsDisplayed()
        compose.onNode(hasSetTextAction() and hasText("old-model")).performTextReplacement("my-model")
        compose.onNode(hasSetTextAction() and hasText("my-model")).assertIsDisplayed()
    }

    @Test fun pluginTogglesKeepUnknownSavedIdsWhileSwitchingInstalledOnes() {
        var selection by mutableStateOf(listOf("missing-skill"))
        compose.setContent { Hosted { PluginToggles("Skills", "No skills yet.",
            listOf(Triple("installed-skill", "Fixture skill", "From the fixture")), selection, { selection = it }) } }
        toggle("Fixture skill").assertIsOff().performClick().assertIsOn()
        assertEquals(listOf("missing-skill", "installed-skill"), selection)
        toggle("Fixture skill").performClick().assertIsOff()
        assertEquals(listOf("missing-skill"), selection)
        compose.onNodeWithText("No skills yet.").assertDoesNotExist()
    }

    private fun render(draft: BotDraft, state: AppState = fixture(), close: () -> Unit = {}) {
        compose.setContent { Hosted { CompositionLocalProvider(LocalDismissModal provides close) { BotEditor(state, draft, store) } } }
    }

    private fun toggle(title: String) = compose.onNode(hasText(title) and isToggleable())

    private fun fixture(): AppState = AppState(Computer("fixture", "Fixture computer", "fixture-key"), loading = false,
        actualConnection = LinkState.Ready(HostRoute.Direct), plugins = PluginListing(emptyList(), emptyList()), hello = buildJsonObject {
            put("backends", buildJsonArray {
                add(buildJsonObject { put("id", "claude"); put("name", "Claude"); put("available", true) })
                add(buildJsonObject { put("id", "codex"); put("name", "Codex"); put("available", false); put("installHint", "Install Codex on your computer.") })
            })
            put("screen", buildJsonObject { put("enabled", true) })
        }, screen = ScreenState(enabled = true, computerUse = true))

    private fun snapshot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("layoutSnapshots") != "true") return
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        File(context.cacheDir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }
}
