package com.codync.android

import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/** Opt-in: a paired, isolated real host whose ACP fixture reports 50,000 of 200,000 tokens. */
class LiveContextUsageTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun reportedSessionUsageOpensDetailsAndSurvivesClosingThem() {
        val arguments = InstrumentationRegistry.getArguments()
        assumeTrue(arguments.getString("liveHost") == "true")
        // Animated avatars keep the live app moving; advance frames only when this flow needs them.
        compose.mainClock.autoAdvance = false
        val bot = "Context meter QA"
        compose.waitUntil(30_000) { compose.mainClock.advanceTimeBy(100); compose.onAllNodesWithText(bot).fetchSemanticsNodes().isNotEmpty() }
        val back = compose.onAllNodes(hasText("Back") or hasContentDescription("Back")).fetchSemanticsNodes()
        if (back.isEmpty()) compose.onNodeWithText(bot).performClick()
        val indicator = "Context window: 25% used"
        compose.waitUntil(30_000) { compose.mainClock.advanceTimeBy(100); compose.onAllNodesWithContentDescription(indicator).fetchSemanticsNodes().isNotEmpty() }
        compose.mainClock.advanceTimeBy(1_000)
        snapshot("context-meter-composer.png")
        compose.onNodeWithContentDescription(indicator).assertIsDisplayed().assertHasClickAction()
        compose.onNodeWithContentDescription(indicator).performClick()
        compose.mainClock.advanceTimeBy(1_000)
        compose.onNodeWithText("Context window").assertIsDisplayed()
        compose.onNodeWithText("25%").assertIsDisplayed()
        compose.onNodeWithText("50,000 / 200,000 tokens").assertIsDisplayed()
        snapshot("context-meter-details.png")
        compose.onNodeWithText("Context window").performTouchInput { click() }
        compose.onNodeWithText("50,000 / 200,000 tokens").assertIsDisplayed()
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitUntil(10_000) { compose.mainClock.advanceTimeBy(100); compose.onAllNodesWithText("Context window").fetchSemanticsNodes().isEmpty() }
        compose.onNodeWithContentDescription(indicator).assertIsDisplayed()
    }

    private fun snapshot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("layoutSnapshots") != "true") return
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        File(context.cacheDir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }
}
