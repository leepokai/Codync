package com.codync.android

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.test.platform.app.InstrumentationRegistry
import android.graphics.Bitmap
import java.io.File
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.Density
import com.codync.android.core.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** Read-only roster fixture: nothing here pairs, saves or sends. */
class LayoutTest {
    @get:Rule val compose = createComposeRule()
    private val state = AppState(loading = false, setupComplete = true, rosterLoaded = true,
        computers = listOf(Computer("fixture", "Acceptance computer", "fixture")),
        connection = LinkState.Ready(HostRoute.Direct), actualConnection = LinkState.Ready(HostRoute.Direct), bots = (1..5).map {
            Bot("bot-$it", "Bot $it", lastMessage = "A recent reply", unread = if (it == 1) 12 else 0)
        })
    private val computers = hasContentDescription("Computers,", substring = true)
    private fun tab(title: String) = hasText(title) and isSelectable()

    @Test fun theRosterShowsFiveRowsAndKeepsActionsOnDemand() {
        compose.setContent { Hosted { CodyncApp(state, viewModel()) } }
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
        compose.onNode(tab("State")).performClick().assertIsSelected()
        compose.onNodeWithText("Widget").assertIsSelected()
        compose.onNodeWithText("Choose your widget").performScrollTo().assertIsDisplayed()
        snapshot("widget-gallery")
        compose.onNodeWithText("Bot 1").assertDoesNotExist()
        compose.onNode(tab("Bots")).performClick().assertIsSelected()
        compose.onNodeWithText("Bot 5").assertIsDisplayed()
    }

    @Test fun largerTextKeepsTheHeaderControlsAndUnreadBadgeAccessible() {
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density, 1.5f)) {
                Hosted { CodyncApp(state, viewModel()) }
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
        compose.onNodeWithText("Acceptance computer · Connected").assertIsDisplayed()
    }

    private fun snapshot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("layoutSnapshots") != "true") return
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val file = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "layout-$name.png")
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }
}
