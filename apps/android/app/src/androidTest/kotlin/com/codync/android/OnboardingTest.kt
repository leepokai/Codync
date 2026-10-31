package com.codync.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.lifecycle.viewmodel.compose.viewModel
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Read-only: the store only receives taps that never pair, skip or sign in. */
class OnboardingTest {
    @get:Rule val compose = createComposeRule()

    @Test fun welcomeLeadsToInstallAndBackReturnsToWelcome() {
        // The welcome crew never stops moving: step the clock by hand so the test can settle.
        compose.mainClock.autoAdvance = false
        compose.setContent { Hosted { CodyncApp(AccountContext(loading = false), emptyMap(), viewModel()) } }
        step()
        compose.onNodeWithText("Your coding agents", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Get started").performClick()
        step()
        compose.onNodeWithText("Install Codync", substring = true).assertIsDisplayed()
        compose.onNodeWithContentDescription("Step 1 of 2").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back").performClick()
        step()
        compose.onNodeWithText("Get started").assertIsDisplayed()
    }

    private fun step() = compose.mainClock.advanceTimeBy(1_000)

    @Test fun installStepsFollowTheComputerAndScanKeepsTheChoice() {
        var skipped = false
        compose.setContent { Hosted { PairingView(AccountContext(loading = false), viewModel(), onSkip = { skipped = true }) } }
        compose.onNodeWithText("Mac").assertIsSelected()
        compose.onNodeWithText("brew install --cask leepokai/codync/codync").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Linux").performClick().assertIsSelected()
        compose.onNodeWithText("codync-host install keeps it running in the background.").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("brew install --cask leepokai/codync/codync").assertDoesNotExist()
        compose.onNodeWithText("Skip").assertDoesNotExist()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Scan the pairing code").assertIsDisplayed()
        compose.onNodeWithContentDescription("Step 2 of 2").assertIsDisplayed()
        compose.onNodeWithText("codync-host pair").assertIsDisplayed()
        compose.onNodeWithText("Or paste a ${BuildConfig.URL_SCHEME}://pair link").performScrollTo().assertIsDisplayed()
        compose.onNodeWithContentDescription("Pair").assertIsNotEnabled()
        compose.onNode(hasSetTextAction()).performTextInput("draft-link")
        compose.onNodeWithContentDescription("Pair").assertIsEnabled()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithText("Linux").assertIsSelected()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Skip").performClick()
        assertTrue(skipped)
    }

    @Test fun macPairingPointsToTheMenuBarApp() {
        compose.setContent { Hosted { PairingView(AccountContext(loading = false), viewModel()) } }
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("On your Mac, click Codync in the menu bar, then Pair iPhone. The same code pairs this phone.").assertIsDisplayed()
        compose.onNodeWithText("codync-host pair").assertDoesNotExist()
    }
}
