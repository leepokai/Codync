package com.codync.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.lifecycle.viewmodel.compose.viewModel
import com.codync.android.core.Computer
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Read-only: the store only receives taps that never pair, skip or sign in. */
class OnboardingTest {
    @get:Rule val compose = createComposeRule()

    @Test fun signedOutWithAccountsTheWelcomeOnlySignsInAndCountsTheWaitingComputers() {
        compose.mainClock.autoAdvance = false
        val signedOut = AccountContext(loading = false, account = AccountState(ready = true, configured = true),
            computers = listOf(Computer("waiting-computer-000001", "Mac mini", "fixture-key"), Computer("waiting-computer-000002", "Linux", "fixture-key")))
        compose.setContent { Hosted { CodyncApp(signedOut, emptyMap(), viewModel()) } }
        step()
        compose.onNodeWithText("Continue with Apple").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Continue with Google").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Sign in to keep using your 2 computers", substring = true).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Get started").assertDoesNotExist()
    }

    @Test fun aFailedRestoreOffersTryAgainWhileSignInWaits() {
        compose.mainClock.autoAdvance = false
        val failed = AccountContext(loading = false,
            account = AccountState(ready = false, configured = true, error = "Couldn't restore sign-in. Check your connection and retry."))
        compose.setContent { Hosted { CodyncApp(failed, emptyMap(), viewModel()) } }
        step()
        // Below the fold: scrolling would wait on a clock this test holds still.
        compose.onNodeWithText("Couldn't restore sign-in. Check your connection and retry.").assertExists()
        compose.onNodeWithContentDescription("Try again").assertIsEnabled()
        compose.onNodeWithText("Continue with Apple").assertIsNotEnabled()
    }

    @Test fun welcomeLeadsToInstallAndBackReturnsToWelcome() {
        // The welcome crew never stops moving: step the clock by hand so the test can settle.
        compose.mainClock.autoAdvance = false
        // Only builds without accounts pair signed out.
        compose.setContent { Hosted { CodyncApp(AccountContext(loading = false, account = AccountState(ready = true, configured = false)), emptyMap(), viewModel()) } }
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
        compose.onNodeWithText("Pairing link or code").performScrollTo().assertIsDisplayed()
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
