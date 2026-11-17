package com.codync.android

import com.codync.android.core.CloudMachine
import com.codync.android.core.CloudOffer
import org.junit.Assert.*
import org.junit.Test

/** Each Codync Cloud error shows where its action was: a key's under it in its key sheet, a cloud computer's on the card,
 * creating's in New cloud computer (iOS `cloudKeyComesBeforeTheComputer`). */
class CloudErrorTest {
    @Test fun keyErrorShowsUnderItsOwnKey() {
        val refused = AccountContext(error = "This key wasn't accepted.", failedAction = cloudKeyAction("boat"))
        assertEquals("This key wasn't accepted.", refused.cloudErrorAt(CloudErrorPlace.Key("boat")))
        assertNull(refused.cloudErrorAt(CloudErrorPlace.Key("instabox")))
        assertNull(refused.cloudErrorAt(CloudErrorPlace.Card))
        assertNull(refused.cloudErrorAt(CloudErrorPlace.Create))
        // Whichever provider's, New cloud computer shows it too (a removal asked from a row's ⋯).
        assertEquals(refused.error, refused.cloudKeyError())
    }

    @Test fun aCloudComputersErrorShowsOnTheCardAndCreatingsInItsSheet() {
        val asleep = AccountContext(error = "Your cloud computer is asleep.", failedAction = CloudMachineAction.DeleteSnapshots.key("cm_1"))
        assertEquals("Your cloud computer is asleep.", asleep.cloudErrorAt(CloudErrorPlace.Card))
        assertNull(asleep.cloudErrorAt(CloudErrorPlace.Create))
        assertNull(asleep.cloudErrorAt(CloudErrorPlace.Key("boat")))
        assertNull(asleep.cloudKeyError())
        val limit = AccountContext(error = "You can have up to 3 cloud computers. Delete one to make another.", failedAction = CLOUD_CREATE)
        assertEquals(limit.error, limit.cloudErrorAt(CloudErrorPlace.Create))
        assertNull(limit.cloudErrorAt(CloudErrorPlace.Card))
        // Anything else is the app's dialog.
        assertNull(CloudErrorPlace.of("cloud"))
        assertNull(CloudErrorPlace.of(null))
    }

    @Test fun eachCloudComputerIsBusyOnItsOwn() {
        val renaming = AccountContext(busy = setOf(CloudMachineAction.Rename.key("cm_1"), CLOUD_CREATE))
        assertTrue(renaming.cloudMachineBusy("cm_1"))
        assertFalse(renaming.cloudMachineBusy("cm_2"))
        assertTrue(AccountContext(busy = setOf(CloudMachineAction.Delete.key("cm_2"))).cloudMachineBusy("cm_2"))
        assertFalse(AccountContext(busy = setOf(CLOUD_CREATE)).cloudMachineBusy("cm_1"))
    }

    @Test fun everyCloudComputerStillPreparingIsListed() {
        fun machine(id: String, state: String, computerId: String?) = CloudMachine(id, "Cloud computer", "boat", "Boat", "EU", state, computerId)
        val offer = CloudOffer(listOf(machine("cm_1", "starting", null), machine("cm_2", "running", "c1"), machine("cm_3", "failed", null),
            machine("cm_4", "running", null)), true)
        assertEquals(setOf("cm_1", "cm_4"), AccountContext(cloudOffer = offer).preparingCloud)
        assertEquals(emptySet<String>(), AccountContext().preparingCloud)
    }
}
