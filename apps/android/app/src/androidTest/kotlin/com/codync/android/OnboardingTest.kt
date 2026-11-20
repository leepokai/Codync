package com.codync.android

import android.content.ClipboardManager
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.CloudComputer
import com.codync.android.core.CloudMachine
import com.codync.android.core.CloudOffer
import com.codync.android.core.Computer
import org.junit.Assert.assertEquals
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

    /** Signed in, past the account's answer unless [loaded] says otherwise. */
    private fun signedIn(vararg computers: CloudComputer, loaded: Boolean = true) = AccountContext(loading = false, cloudLoaded = loaded,
        cloudComputers = computers.toList(), account = AccountState(ready = true, configured = true, userId = "user",
            accounts = listOf(SavedAccount("user", "session", "owner@example.com", null))))

    @Test fun anAccountWithAComputerChoosesBetweenAskingItAndPairingANewOne() {
        val computer = CloudComputer("account-computer", "Desk", "account-key", online = true)
        compose.setContent { Hosted { CodyncApp(signedIn(computer), emptyMap(), viewModel()) } }
        compose.onNodeWithText("Connect a computer").assertIsDisplayed()
        compose.onNodeWithText("Desk").assertIsDisplayed()
        compose.onNodeWithText("Online · In your account").assertIsDisplayed()
        compose.onNodeWithText("Ask for access").assertIsEnabled()
        compose.onNodeWithText("Skip").assertIsDisplayed()
        compose.onNodeWithText("Install Codync\non your computer").assertDoesNotExist()
        compose.onNodeWithText("Pair a new computer").performClick()
        compose.onNodeWithText("Install Codync\non your computer").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithText("Connect a computer").assertIsDisplayed()
    }

    @Test fun anEarlierApprovalIsReplacedByAskingNotRevokedFirst() {
        val computer = CloudComputer("account-computer", "Desk", "account-key", online = true, access = "granted")
        compose.setContent { Hosted { CodyncApp(signedIn(computer), emptyMap(), viewModel()) } }
        compose.onNodeWithText("Ask for access").assertIsDisplayed()
        compose.onNodeWithText("Revoke").assertDoesNotExist()
    }

    @Test fun anAccountWithNoComputerGoesStraightToInstallWithTheLinkToSend() {
        compose.setContent { Hosted { CodyncApp(signedIn(), emptyMap(), viewModel()) } }
        compose.onNodeWithText("Install Codync\non your computer").assertIsDisplayed()
        compose.onNodeWithText("Connect a computer").assertDoesNotExist()
        compose.onNodeWithText("Copy website link").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Sign in there with owner@example.com and it shows up here.").assertIsDisplayed()
    }

    @Test fun setupWaitsForTheAccountsAnswerBeforeChoosingAPage() {
        compose.setContent { Hosted { CodyncApp(signedIn(loaded = false), emptyMap(), viewModel()) } }
        compose.onNodeWithText("Install Codync\non your computer").assertDoesNotExist()
        compose.onNodeWithText("Connect a computer").assertDoesNotExist()
    }

    @Test fun aComputerJoiningTheAccountDoesNotPullPairingBackToTheChoice() {
        val context = mutableStateOf(signedIn())
        compose.setContent { Hosted { CodyncApp(context.value, emptyMap(), viewModel()) } }
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Scan the pairing code").assertIsDisplayed()
        compose.runOnUiThread { context.value = signedIn(CloudComputer("account-computer", "Desk", "account-key", online = true)) }
        compose.waitForIdle()
        compose.onNodeWithText("Connect a computer").assertDoesNotExist()
        compose.onNodeWithText("Scan the pairing code").assertIsDisplayed()
    }

    @Test fun theChoicePageSkipsSetup() {
        val computer = CloudComputer("account-computer", "Desk", "account-key", online = true)
        var skipped = false
        compose.setContent { Hosted { ChooseComputerView(signedIn(computer), listOf(computer), {}, {}, { skipped = true }) {} } }
        compose.onNodeWithText("Skip").performClick()
        assertTrue(skipped)
    }

    private val preparing = CloudMachine("cm_1", "Cloud computer", "boat", "Boat", "EU", "starting")

    /** [context] with Codync Cloud open to the account (or not, [available]) and its cloud computers. */
    private fun withCloud(context: AccountContext, vararg machines: CloudMachine, available: Boolean = true) =
        context.copy(cloudOffer = CloudOffer(machines.toList(), available))

    @Test fun codyncCloudOpenToTheAccountOffersACloudComputerOnInstallAndChoose() {
        val desk = CloudComputer("account-computer", "Desk", "account-key", online = true)
        val context = mutableStateOf(withCloud(signedIn()))
        compose.setContent { Hosted { CodyncApp(context.value, emptyMap(), viewModel()) } }
        compose.onNodeWithText("Install Codync\non your computer").assertIsDisplayed()
        compose.onNodeWithText("Start a cloud computer").performScrollTo().assertIsDisplayed()
        compose.runOnUiThread { context.value = withCloud(signedIn(desk)) }
        compose.onNodeWithText("Connect a computer").assertIsDisplayed()
        compose.onNodeWithText("Start a cloud computer").performScrollTo().assertIsDisplayed()
        compose.runOnUiThread { context.value = withCloud(signedIn(desk), available = false) }
        compose.onNodeWithText("Connect a computer").assertIsDisplayed()
        compose.onNodeWithText("Start a cloud computer").assertDoesNotExist()
    }

    @Test fun pairingAnotherComputerFromTheAccountHasNoCloudComputerRow() {
        compose.setContent { Hosted { CodyncApp(withCloud(signedIn()), emptyMap(), viewModel()) } }
        compose.onNodeWithContentDescription("Switch account").performClick()
        compose.onNodeWithText("Scan a computer's code").performClick()
        // The install page under the sheets and the sheet's own: only the first-run page offers a cloud computer.
        compose.onAllNodesWithContentDescription("Step 1 of 2").assertCountEquals(2)
        compose.onAllNodesWithText("Start a cloud computer").assertCountEquals(1)
    }

    @Test fun aCloudComputerStillPreparingTakesOverSetupUntilItIsLeftWithBack() {
        val context = mutableStateOf(withCloud(signedIn(), preparing))
        compose.setContent { Hosted { CodyncApp(context.value, emptyMap(), viewModel()) } }
        compose.onNodeWithText("Starting your cloud computer").assertIsDisplayed()
        compose.onNodeWithText("Cloud computer · Runs on Boat · EU").assertIsDisplayed()
        compose.onNodeWithText("Preparing your cloud computer…").assertIsDisplayed()
        compose.onNodeWithText("Back").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Install Codync\non your computer").assertDoesNotExist()
        // What its Back leaves (`passCloud`): setup goes on from the install page.
        compose.runOnUiThread { context.value = context.value.copy(passedCloud = setOf("cm_1")) }
        compose.onNodeWithText("Install Codync\non your computer").assertIsDisplayed()
        compose.onNodeWithText("Starting your cloud computer").assertDoesNotExist()
    }

    @Test fun aCloudComputerStartedHereThatFailedSaysSo() {
        val context = mutableStateOf(withCloud(signedIn(), preparing.copy(state = "failed")))
        compose.setContent { Hosted { CodyncApp(context.value, emptyMap(), viewModel()) } }
        // One that failed elsewhere isn't waited for.
        compose.onNodeWithText("Install Codync\non your computer").assertIsDisplayed()
        compose.runOnUiThread { context.value = context.value.copy(startedCloud = true) }
        compose.onNodeWithText("Cloud computer failed").assertIsDisplayed()
        compose.onNodeWithText("Preparing your cloud computer…").assertDoesNotExist()
    }

    @Test fun theWaitPageLeavesWithBackOrSkip() {
        var back = false
        var skipped = false
        compose.setContent { Hosted { CloudStartView(preparing.copy(state = "failed"), { back = true }, { skipped = true }) {} } }
        compose.onNodeWithText("Back").performScrollTo().performClick()
        compose.onNodeWithText("Skip").performClick()
        assertTrue(back && skipped)
    }

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

    @Test fun theWebsiteLinkButtonCopiesTheLinkAndSaysSo() {
        // The page's bot never stops moving, and "Copied" lasts only a moment: step the clock by hand.
        compose.mainClock.autoAdvance = false
        compose.setContent { Hosted { PairingView(AccountContext(loading = false), viewModel()) } }
        step()
        compose.onNodeWithText("Copy website link").performClick()
        compose.mainClock.advanceTimeBy(500)
        compose.onNodeWithText("Copied").assertExists()
        val clipboard = InstrumentationRegistry.getInstrumentation().targetContext.getSystemService(ClipboardManager::class.java)
        assertEquals("https://www.codync.dev", clipboard.primaryClip?.getItemAt(0)?.text?.toString())
    }

    @Test fun macPairingPointsToTheMenuBarApp() {
        compose.setContent { Hosted { PairingView(AccountContext(loading = false), viewModel()) } }
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("On your Mac, click Codync in the menu bar, then Pair phone. The same code pairs this phone.").assertIsDisplayed()
        compose.onNodeWithText("codync-host pair").assertDoesNotExist()
    }
}
