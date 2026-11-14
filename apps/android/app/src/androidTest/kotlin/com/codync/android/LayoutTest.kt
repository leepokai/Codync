package com.codync.android

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.test.platform.app.InstrumentationRegistry
import android.graphics.Bitmap
import java.io.File
import androidx.compose.ui.test.*
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.Density
import com.codync.android.core.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** Read-only roster fixture: nothing here pairs, saves or sends. */
class LayoutTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val computer = Computer("fixture-computer-00001", "Acceptance computer", "fixture")
    private val app = AccountContext(loading = false, setupComplete = true, account = AccountState(ready = true), computers = listOf(computer))
    private val state = AppState(computer, loading = false, rosterLoaded = true,
        connection = LinkState.Ready(HostRoute.Direct), actualConnection = LinkState.Ready(HostRoute.Direct), bots = (1..5).map {
            Bot("bot-$it", "Bot $it", lastMessage = "A recent reply", unread = if (it == 1) 12 else 0)
        })
    private val computers = hasContentDescription("Computers,", substring = true)
    private fun tab(title: String) = hasText(title) and isSelectable()

    @Test fun theRosterShowsFiveRowsAndKeepsActionsOnDemand() = compose.withPreviewAccounts(computer) { accounts ->
        compose.setContent { Hosted { CodyncApp(app, mapOf(computer.id to state), accounts) } }
        (1..5).forEach { compose.onNodeWithText("Bot $it").assertIsDisplayed() }
        compose.onNodeWithText("12").assertIsDisplayed()
        snapshot("roster")
        compose.onNodeWithText("New bot").assertDoesNotExist()
        compose.onNodeWithText("Account").assertDoesNotExist()
        compose.onNodeWithContentDescription("Switch account").performClick()
        compose.onNodeWithText("Account").assertIsDisplayed()
        compose.onNodeWithContentDescription("Close").performClick()
        compose.onNodeWithText("Account").assertDoesNotExist()
        compose.onNodeWithContentDescription("New").performClick()
        compose.onNodeWithText("New group chat").assertIsDisplayed()
        compose.onNodeWithText("New bot").performClick()
        compose.onNodeWithContentDescription("Create").assertIsDisplayed()
        compose.onNode(hasSetTextAction() and hasText("Name your bot")).assertIsDisplayed()
        compose.onNodeWithContentDescription("Close").performClick()
        compose.onNodeWithContentDescription("Create").assertDoesNotExist()
        // The State tab's widget previews never stop moving: step the clock by hand there.
        compose.mainClock.autoAdvance = false
        compose.onNode(tab("State")).performClick()
        compose.mainClock.advanceTimeBy(1_000)
        compose.onNode(tab("State")).assertIsSelected()
        compose.onNodeWithText("Widget").assertIsSelected()
        compose.onNodeWithText("Choose your widget").performScrollTo()
        compose.mainClock.advanceTimeBy(500)
        compose.onNodeWithText("Choose your widget").assertIsDisplayed()
        snapshot("widget-gallery")
        compose.onNodeWithText("Bot 1").assertDoesNotExist()
        compose.onNode(tab("Bots")).performClick()
        compose.mainClock.advanceTimeBy(1_000)
        compose.mainClock.autoAdvance = true
        compose.onNode(tab("Bots")).assertIsSelected()
        compose.onNodeWithText("Bot 5").assertIsDisplayed()
    }

    @Test fun aLinkPicksItsTabEvenWhenItArrivesWithTheTabs() = compose.withPreviewAccounts(computer) { accounts ->
        // A cold start: the link is counted while the account loads, before the tabs first show.
        val context = mutableStateOf(app.copy(loading = true))
        compose.setContent { Hosted { CodyncApp(context.value, mapOf(computer.id to state), accounts) } }
        // The State tab's widget previews never stop moving: step the clock by hand.
        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { context.value = app.copy(navigationRequest = 1, navigationPage = LinkPage.Usage) }
        compose.mainClock.advanceTimeBy(1_000)
        compose.onNode(tab("State")).assertIsSelected()
        compose.runOnUiThread { context.value = app.copy(navigationRequest = 2, navigationPage = LinkPage.Computers) }
        compose.mainClock.advanceTimeBy(1_000)
        compose.mainClock.autoAdvance = true
        compose.onNode(tab("Bots")).assertIsSelected()
        compose.onNodeWithText("Computer management").assertIsDisplayed()
    }

    @Test fun largerTextKeepsTheHeaderControlsAndUnreadBadgeAccessible() = compose.withPreviewAccounts(computer) { accounts ->
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density, 1.5f)) {
                Hosted { CodyncApp(app, mapOf(computer.id to state), accounts) }
            }
        }
        val account = compose.onNodeWithContentDescription("Switch account").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        val summary = compose.onNode(computers).assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        val new = compose.onNodeWithContentDescription("New").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
        compose.onNodeWithContentDescription("12 unread").assertIsDisplayed()
        assertTrue(account.right <= summary.left)
        assertTrue(summary.right <= new.left)
        snapshot("large-text")
        compose.onNode(computers).performClick()
        compose.onNodeWithText("Manage computers").assertIsDisplayed()
        compose.onNodeWithText("Connected").assertIsDisplayed()
    }

    private fun snapshot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("layoutSnapshots") != "true") return
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val file = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "layout-$name.png")
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }
}
