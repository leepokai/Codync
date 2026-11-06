package com.codync.android

import android.graphics.Bitmap
import android.os.Build
import android.view.KeyEvent
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import com.codync.android.design.CodyncSheet
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import java.io.File

/** The real root's sheets and Android Back dispatch, on read-only fixtures. */
class ScreenPresentationTest {
    @get:Rule val compose = createComposeRule()
    private val bot = Bot("presentation-fixture", "Scout")
    private fun fixture() = AppState(contextId = "presentation-fixture", loading = false,
        setupComplete = true, rosterLoaded = true, bots = listOf(bot),
        computers = listOf(Computer("fixture-computer", "Fixture computer", "fixture-key")),
        account = AccountState(ready = true, configured = true, multiple = true, userId = "fixture-account",
            accounts = listOf(SavedAccount("fixture-account", "fixture-session", "alex@example.test", null))))

    @Before fun isolatedEmulatorOnly() { assumeTrue(Build.PRODUCT.startsWith("sdk")) }

    @Test fun settingsIsATabAndBackClosesComputerManagementOneLayerAtATime() {
        compose.setContent { Hosted { CodyncApp(fixture(), viewModel()) } }
        compose.onNodeWithContentDescription("Switch account").performClick()
        compose.onNodeWithText("Account").assertIsDisplayed()
        snapshot("account-sheet")
        back()
        compose.onNodeWithText("Account").assertDoesNotExist()
        compose.onNodeWithText("Settings").performClick()
        compose.onNodeWithText("Pair a computer").assertDoesNotExist()
        compose.onNodeWithText("App language").assertIsDisplayed()
        compose.onNodeWithText("Bots").performClick()
        compose.computerAction("Manage computers")
        compose.onNodeWithText("Computer management").assertIsDisplayed()
        compose.onNodeWithText("App language").assertDoesNotExist()
        compose.onNodeWithText("Pair a computer").performScrollTo().performClick()
        compose.onNodeWithText("Install Codync", substring = true).assertIsDisplayed()
        back()
        compose.onNodeWithText("Install Codync", substring = true).assertDoesNotExist()
        compose.onNodeWithText("Pair a computer").assertIsDisplayed()
        back()
        compose.onNodeWithText("Computer management").assertDoesNotExist()
        compose.onNodeWithText(bot.name).assertIsDisplayed()
    }

    @Test fun closingTheEditorDropsItsDraftAndANewGenerationRetiresIt() {
        val state = mutableStateOf(fixture())
        compose.setContent { Hosted { CodyncApp(state.value, viewModel()) } }
        compose.rosterAction(bot.name, "Edit profile")
        compose.onNodeWithContentDescription("Save").assertIsNotEnabled()
        compose.onNode(hasSetTextAction() and hasText(bot.name)).performTextReplacement("Discard this draft")
        snapshot("bot-settings-sheet")
        compose.onNodeWithContentDescription("Close").performClick()
        compose.onNodeWithContentDescription("Save").assertDoesNotExist()
        compose.rosterAction(bot.name, "Edit profile")
        compose.onNode(hasSetTextAction() and hasText(bot.name)).assertIsDisplayed()
        compose.runOnUiThread { state.value = state.value.copy(generation = state.value.generation + 1) }
        compose.onNodeWithContentDescription("Save").assertDoesNotExist()
    }

    @Test fun onlyABotsTouchAndHoldMenuStartsANewSession() {
        val group = Bot("presentation-group", "Crew", kind = "group", members = listOf(bot.id))
        compose.setContent { Hosted { CodyncApp(fixture().copy(bots = listOf(bot, group)), viewModel()) } }
        compose.rosterAction(bot.name, "New session")
        compose.onNodeWithText("Start a new session?").assertIsDisplayed()
        compose.onNodeWithText("Cancel").performClick()
        compose.onNodeWithText("Start a new session?").assertDoesNotExist()
        compose.rosterActions(group.name)
        compose.onNodeWithText("Edit group").assertIsDisplayed()
        compose.onNodeWithText("New session").assertDoesNotExist()
    }

    @Test fun largeTextKeepsSettingsActionsAndCompactOptionsWithinTheSheet() {
        val draft = BotDraft(id = bot.id, name = bot.name, cwd = "/projects/A-long-project-name")
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density, 1.5f)) {
                Hosted { CodyncSheet(true, {}) { BotEditor(fixture(), draft, viewModel()) } }
            }
        }
        val title = compose.onNodeWithText("Settings").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        val save = compose.onNodeWithContentDescription("Save").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        val close = compose.onNodeWithContentDescription("Close").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        assertTrue(title.right < save.left && save.right <= close.left)
        val screen = compose.onRoot().fetchSemanticsNode().boundsInRoot
        listOf("Agent: Claude", "Workspace", "Permissions: Approve automatically").forEach { label ->
            val row = compose.onNodeWithContentDescription(label).performScrollTo().assertIsDisplayed().fetchSemanticsNode().boundsInRoot
            assertTrue(row.left >= screen.left && row.right <= screen.right)
        }
        compose.onNode(hasText("Notifications") and isToggleable()).performScrollTo().assertIsOn().performClick().assertIsOff()
        snapshot("bot-settings-large-text")
    }

    private fun back() = InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(KeyEvent.KEYCODE_BACK)
    private fun snapshot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("layoutSnapshots") != "true") return
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val screenshot = instrumentation.uiAutomation.takeScreenshot()
        File(instrumentation.targetContext.cacheDir, "$name.png").outputStream().use { screenshot.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }
}
