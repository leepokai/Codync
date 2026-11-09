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
    private fun machine(state: String, computerId: String? = "cloud", name: String = "Cloud computer") =
        CloudMachine("cm_1", name, "boat", "Boat", "EU", state, computerId)
    private fun placement(state: String) = CloudPlacement("boat", "Boat", "EU", state)
    private fun cloudComputer(state: String) = CloudComputer("cloud", "Cloud computer", "key", online = false, access = "none", cloud = placement(state))
    private fun fixture(offer: CloudOffer?, computers: List<CloudComputer> = emptyList()) = AccountContext(loading = false, setupComplete = true,
        account = account, contextId = "cloud-fixture", cloudOffer = offer, cloudComputers = computers)
    /** The cloud computer as this phone uses it. */
    private fun used(state: String) = AppState(Computer("cloud", "Cloud computer", "key"), loading = false, cloudHosting = placement(state))

    @Before fun isolatedEmulatorOnly() { assumeTrue(Build.PRODUCT.startsWith("sdk")) }

    @Test fun aSleepingCloudComputerShowsItsProviderAndCanBeDeleted() {
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(listOf(machine("stopped")), true), listOf(cloudComputer("stopped"))), emptyList(), viewModel(), {}, {}, {}) } }
        // Its account computer says where it runs.
        compose.onNodeWithText("Runs on Boat · EU").assertIsDisplayed()
        compose.onNodeWithText("Sleeping · wakes when you message").assertIsDisplayed()
        compose.onNodeWithText("Boat · EU · Sleeping").performScrollTo().assertIsDisplayed()
        compose.onNodeWithContentDescription("More").performClick()
        compose.onNodeWithText("Rename…").assertIsDisplayed()
        compose.onNodeWithText("Boat API key").assertIsDisplayed()
        compose.onNodeWithText("Delete snapshots").assertIsDisplayed()
        snapshot("sleeping")
        compose.onNodeWithText("Delete cloud computer").performClick()
        compose.onNodeWithText("Delete Cloud computer?").assertIsDisplayed()
        snapshot("delete")
    }

    @Test fun aProviderThatKeepsNoSnapshotsOffersOnlyDeletingTheComputer() {
        val instabox = CloudMachine("cm_1", "Builds", "instabox", "InstaBox", "US", "stopped", "cloud", snapshots = false)
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(listOf(instabox), true)), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithText("Builds").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("InstaBox · US · Sleeping").assertIsDisplayed()
        compose.onNodeWithContentDescription("More").performClick()
        compose.onNodeWithText("Delete cloud computer").assertIsDisplayed()
        compose.onNodeWithText("Delete snapshots").assertDoesNotExist()
    }

    @Test fun aNewCloudComputerIsPreparedUntilItJoinsTheAccount() {
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(listOf(machine("starting", computerId = null)), true)), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithText("Boat · EU · Preparing your cloud computer…").performScrollTo().assertIsDisplayed()
        snapshot("preparing")
    }

    private val setup = CloudKeySetup("https://boat.dev/dashboard", "boat env new codync")
    private fun boat(key: CloudCredential? = null) = CloudProvider("boat", "Boat", "EU", "boat_…", setup, key)

    @Test fun aProviderKeyComesFirst() {
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(emptyList(), available = true, providers = listOf(boat()))), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithContentDescription("New cloud computer").performScrollTo().performClick()
        compose.onNodeWithText("EU · Add a key first").assertIsDisplayed()
        // Only a usable key has its row's ⋯.
        compose.onNodeWithTag("cloudProviderKey.boat").assertDoesNotExist()
        compose.onNodeWithContentDescription("Boat").performClick()
        // Only that provider's key.
        compose.onNodeWithText("Boat API key").assertIsDisplayed()
        compose.onNodeWithContentDescription("Boat API key").assertIsDisplayed()
        compose.onNodeWithText("How to make a key").assertIsDisplayed().performClick()
        compose.onNodeWithText("boat env new codync").assertIsDisplayed()
        compose.onNodeWithText("Open Boat").assertExists()
        // Boat has no free plan.
        compose.onNodeWithText("Get a free sandbox").assertDoesNotExist()
        snapshot("key-sheet")
    }

    @Test fun plusOffersEveryProviderAndAKeyToAddFirst() {
        val key = CloudCredential("boat_…9abc")
        val free = CloudProviderOffer("Get a free sandbox", "InstaBox's free plan includes one machine (4 vCPU, 8 GiB).", "https://instabox.sh/pricing")
        val instabox = CloudProvider("instabox", "InstaBox", "US", "ibk_…", setup, offer = free)
        val offer = CloudOffer(listOf(machine("stopped")), available = true, providers = listOf(boat(key), instabox))
        compose.setContent { Hosted { ComputersView(fixture(offer), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithContentDescription("New cloud computer").performScrollTo().performClick()
        compose.onNodeWithContentDescription("Name your cloud computer").assertIsDisplayed()
        compose.onNodeWithText("Cloud computer 2", useUnmergedTree = true).assertIsDisplayed()
        compose.onNodeWithContentDescription("Boat").assertIsSelected()
        compose.onNodeWithText("EU · boat_…9abc").assertIsDisplayed()
        // Without a key, InstaBox points to its free plan.
        compose.onNodeWithText("US · Free sandbox available").assertIsDisplayed()
        compose.onNodeWithContentDescription("Create").assertIsEnabled()
        compose.onNodeWithTag("cloudProviderKey.instabox").assertDoesNotExist()
        snapshot("new")
        compose.onNodeWithContentDescription("InstaBox").performClick()
        compose.onNodeWithText("InstaBox API key").assertIsDisplayed()
        compose.onNodeWithText("InstaBox's free plan includes one machine (4 vCPU, 8 GiB).").assertIsDisplayed()
        compose.onNodeWithText("Get a free sandbox").assertIsDisplayed()
        compose.onNodeWithText("How to make a key").assertExists()
        compose.onNodeWithText("Boat API key").assertDoesNotExist()
    }

    @Test fun aSavedKeyOpensFromItsRowsMenu() {
        val offer = CloudOffer(emptyList(), available = true, providers = listOf(boat(CloudCredential("boat_…9abc"))))
        compose.setContent { Hosted { ComputersView(fixture(offer), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithContentDescription("New cloud computer").performScrollTo().performClick()
        compose.onNodeWithTag("cloudProviderKey.boat").assertIsDisplayed().performClick()
        compose.onNodeWithText("Remove key").assertIsDisplayed()
        snapshot("provider-menu")
        compose.onNodeWithText("Boat API key").performClick()
        compose.onNodeWithText("Boat API key").assertIsDisplayed()
        compose.onNodeWithContentDescription("Replace key").assertIsDisplayed()
        compose.onNodeWithContentDescription("Remove key").assertIsDisplayed()
    }

    @Test fun aKeyAComputerRunsOnStaysAndAFailedRemovalSaysWhy() {
        val instabox = CloudProvider("instabox", "InstaBox", "US", "ibk_…", setup, CloudCredential("ibk_…0001"))
        val offer = CloudOffer(listOf(machine("stopped")), available = true, providers = listOf(boat(CloudCredential("boat_…9abc")), instabox))
        val failed = fixture(offer).copy(error = "Couldn't reach Codync Cloud.", failedAction = cloudKeyAction("instabox"))
        compose.setContent { Hosted { ComputersView(failed, emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithContentDescription("New cloud computer").performScrollTo().performClick()
        // A removal asked from InstaBox's ⋯ fails in New cloud computer, where it was asked.
        compose.onNodeWithText("Couldn't reach Codync Cloud.").assertIsDisplayed()
        // Boat runs a computer: its ⋯ opens its key, and offers no Remove key.
        compose.onNodeWithTag("cloudProviderKey.boat").performClick()
        compose.onNodeWithText("Boat API key").assertIsDisplayed()
        compose.onNodeWithText("Remove key").assertDoesNotExist()
    }

    @Test fun aKeyTheProviderTurnsDownCantBePickedAndOpensItsKey() {
        val expired = CloudCredential("boat_…9abc", problem = "This key has expired. Make a new one.", blocking = true)
        val offer = CloudOffer(emptyList(), available = true, providers = listOf(boat(expired)))
        compose.setContent { Hosted { ComputersView(fixture(offer), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithContentDescription("New cloud computer").performScrollTo().performClick()
        compose.onNodeWithContentDescription("Create").assertIsNotEnabled()
        compose.onNodeWithTag("cloudProviderKey.boat").assertDoesNotExist()
        compose.onNodeWithContentDescription("Boat").performClick()
        compose.onNodeWithText("Boat API key").assertIsDisplayed()
    }

    @Test fun aPlanTheProviderTurnsDownCanBeTriedAgain() {
        val unpaid = CloudCredential("boat_…9abc", problem = "This account needs an active plan.")
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(emptyList(), available = true, providers = listOf(boat(unpaid)))), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithContentDescription("New cloud computer").performScrollTo().performClick()
        compose.onNodeWithText("This account needs an active plan.").assertIsDisplayed()
        compose.onNodeWithContentDescription("Boat").assertIsSelected()
        compose.onNodeWithContentDescription("Create").assertIsEnabled()
    }

    @Test fun plusAtTheLimitSaysWhyAndKeepsTheKeys() {
        val three = (1..3).map { machine("running", name = "Cloud computer $it").copy(cloudComputerId = "cm_$it") }
        val instabox = CloudProvider("instabox", "InstaBox", "US", "ibk_…", setup, CloudCredential("ibk_…0001"))
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(three, true, limit = 3, providers = listOf(boat(CloudCredential("boat_…9abc")), instabox))), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithContentDescription("New cloud computer").performScrollTo().performClick()
        compose.onNodeWithText("You can have up to 3 cloud computers. Delete one to make another.").assertIsDisplayed()
        compose.onNodeWithContentDescription("Create").assertIsNotEnabled()
        // A key no computer uses can still be replaced or removed.
        compose.onNodeWithTag("cloudProviderKey.instabox").performClick()
        compose.onNodeWithText("Remove key").assertIsDisplayed()
    }

    @Test fun aKeyTheProviderTurnsDownSaysWhatToFix() {
        val key = CloudCredential("boat_…9abc", problem = "This key has expired. Make a new one.", blocking = true)
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(listOf(machine("stopped")), true, providers = listOf(boat(key)))), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithText("Boat · EU · This key has expired. Make a new one.").performScrollTo().assertIsDisplayed()
        compose.onNodeWithContentDescription("More").performClick()
        compose.onNodeWithText("Boat API key").performClick()
        compose.onNodeWithText("This key has expired. Make a new one.").assertIsDisplayed()
        compose.onNodeWithText("boat_…9abc").assertIsDisplayed()
        snapshot("key-refused")
    }

    @Test fun aCloudOffForThisAccountIsHidden() {
        compose.setContent { Hosted { ComputersView(fixture(CloudOffer(emptyList(), available = false)), emptyList(), viewModel(), {}, {}, {}) } }
        compose.onNodeWithContentDescription("New cloud computer").assertDoesNotExist()
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
