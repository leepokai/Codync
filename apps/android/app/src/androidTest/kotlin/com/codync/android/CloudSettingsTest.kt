package com.codync.android

import android.graphics.Bitmap
import android.os.Build
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import java.io.File
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test

/** Codync Cloud from fixture state: its computer's rows, Computers → Codync Cloud and the update cards. Read-only. */
class CloudSettingsTest {
    @get:Rule val compose = createComposeRule()
    private val account = AccountState(ready = true, configured = true, userId = "fixture",
        accounts = listOf(SavedAccount("fixture", "fixture-session", "alex@example.test", null)))
    private fun machine(state: String, computerId: String? = "cloud") = CloudMachine("boat", "Boat", "EU", state, "cm_1", computerId)
    private fun cloudComputer(state: String) = CloudComputer("cloud", "Cloud computer", "key", online = false, access = "none", cloud = machine(state))
    private fun fixture(offer: CloudOffer?, computers: List<CloudComputer> = emptyList()) = AccountContext(loading = false, setupComplete = true,
        account = account, contextId = "cloud-fixture", cloudOffer = offer, cloudComputers = computers)
    /** The cloud computer as this phone uses it. */
    private fun used(state: String) = AppState(Computer("cloud", "Cloud computer", "key"), loading = false, cloudHosting = machine(state))

    @Before fun isolatedEmulatorOnly() { assumeTrue(Build.PRODUCT.startsWith("sdk")) }

    @Test fun aSleepingCloudComputerShowsItsProviderAndCanBeDeleted() {
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(machine("stopped"), true), listOf(cloudComputer("stopped"))), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithText("Codync Cloud · Boat · EU").assertIsDisplayed()
        compose.onNodeWithText("Sleeping · wakes when you message").assertIsDisplayed()
        compose.onNodeWithText("Runs on Boat · EU").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Sleeping").assertExists()
        compose.onNodeWithText("Delete snapshots").performScrollTo().assertIsDisplayed()
        snapshot("sleeping")
        compose.onNodeWithText("Delete cloud computer").performScrollTo().performClick()
        compose.onNodeWithText("Delete your cloud computer?").assertIsDisplayed()
        snapshot("delete")
    }

    @Test fun aNewCloudComputerIsPreparedUntilItJoinsTheAccount() {
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(machine("starting", computerId = null), true)), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithText("Preparing your cloud computer…").performScrollTo().assertIsDisplayed()
        snapshot("preparing")
    }

    private val setup = CloudKeySetup("https://boat.dev/dashboard", "boat env new codync")
    private fun boat(key: CloudCredential? = null) = CloudProvider("boat", "Boat", "EU", "boat_…", setup, key)

    @Test fun aProviderKeyComesFirst() {
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(null, available = true, providers = listOf(boat()))), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithText("Connect one").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Create cloud computer").assertDoesNotExist()
        snapshot("providers-none")
        compose.onNodeWithText("Cloud providers").performClick()
        compose.onNodeWithText("How to make a key").assertIsDisplayed().performClick()
        compose.onNodeWithText("boat env new codync").assertIsDisplayed()
        compose.onNodeWithText("Open Boat").assertExists()
        snapshot("providers-sheet")
    }

    @Test fun onlyAnAccountThatMayHaveOneCanCreateIt() {
        val offer = CloudOffer(null, available = true, providers = listOf(boat(CloudCredential("boat_…9abc"))))
        compose.setContent { Hosted { ComputersView(fixture(offer), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithText("Boat").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Create cloud computer").performScrollTo().assertIsDisplayed()
        snapshot("create")
    }

    @Test fun twoConnectedProvidersAskWhereToCreateIt() {
        val key = CloudCredential("boat_…9abc")
        val offer = CloudOffer(null, available = true, providers = listOf(boat(key), boat(key).copy(provider = "nimbus", name = "Nimbus")))
        compose.setContent { Hosted { ComputersView(fixture(offer), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithText("Create cloud computer").performScrollTo().performClick()
        compose.onNodeWithText("On Boat").assertIsDisplayed()
        compose.onNodeWithText("On Nimbus").assertIsDisplayed()
        snapshot("create-menu")
    }

    @Test fun aKeyTheProviderTurnsDownSaysWhatToFix() {
        val key = CloudCredential("boat_…9abc", problem = "This Boat key has expired. Make a new one.")
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(machine("stopped"), true, listOf(boat(key)))), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithText("Needs attention").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Cloud providers").performClick()
        compose.onNodeWithText("This Boat key has expired. Make a new one.").assertIsDisplayed()
        compose.onNodeWithText("boat_…9abc").assertIsDisplayed()
        snapshot("key-refused")
    }

    @Test fun aCloudOffForThisAccountIsHidden() {
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(null, available = false)), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithText("Create cloud computer").assertDoesNotExist()
        compose.onNodeWithText("Codync Cloud").assertDoesNotExist()
    }

    @Test fun aCloudComputerBehindThisAppUpdatesItself() {
        val state = used("running").copy(connection = LinkState.Ready(HostRoute.Relay), hello = buildJsonObject { put("version", "2.0.0") })
        compose.setContent { Hosted { UpdateNeededCard(state, requireNotNull(state.mismatch), {}) } }
        compose.onNodeWithText("Your cloud computer is updating…").assertIsDisplayed()
        snapshot("updating")
    }

    @Test fun theCloudTurningThisAppAwayShowsItsReason() {
        val state = used("running").copy(appFloor = "Update Codync to keep using it: this version has a security problem.")
        compose.setContent { Hosted { UpdateNeededCard(state, requireNotNull(state.mismatch), {}) } }
        compose.onNodeWithText("Update this app").assertIsDisplayed()
        compose.onNodeWithText("Update Codync to keep using it: this version has a security problem.").assertIsDisplayed()
        snapshot("app-floor")
    }

    private fun snapshot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("layoutSnapshots") != "true") return
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val file = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "cloud-$name.png")
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }
}
