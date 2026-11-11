package com.codync.android

import com.codync.android.core.CallQuality
import com.codync.android.core.CloudError
import com.codync.android.core.HostError
import com.codync.android.core.PairingProblem
import com.codync.android.core.VoiceFailureKind
import com.codync.android.core.VoiceStuck
import java.io.IOException
import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

class AnalyticsTest {
    @Test fun `the computer tag matches the host's hash`() {
        // sha256("codync-analytics:<id>")[0..8], as host/src/analytics.rs and the iPhone compute it.
        assertEquals("fbfcd4f4fe9f876c", Analytics.computerTag("tZ1WMzsVltt-fj7mUDwZsg"))
    }

    @Test fun `event properties encode as JSON booleans numbers and catalog strings`() {
        fun json(event: AnalyticsEvent) = JsonObject(event.properties).toString()
        assertEquals("""{"granted":true,"source":"automatic"}""", json(AnalyticsEvent.NotificationsAnswered(true, PromptSource.Automatic)))
        assertEquals("""{"notifications":"not_asked","widgets":2}""", json(AnalyticsEvent.AppOpened(NotificationAccess.NotAsked, 2)))
        // A widget count the launcher couldn't give is left out, not sent as null.
        assertEquals("""{"notifications":"allowed"}""", json(AnalyticsEvent.AppOpened(NotificationAccess.Allowed, null)))
        assertEquals("""{"setting":"live_activities","on":false}""", json(AnalyticsEvent.SettingChanged(PhoneSetting.LiveActivities, false)))
        assertEquals("""{"via":"link","computer":"fbfcd4f4fe9f876c"}""", json(AnalyticsEvent.ComputerPaired(PairedVia.Link, "tZ1WMzsVltt-fj7mUDwZsg")))
        assertEquals("{}", json(AnalyticsEvent.SignedOut))
        assertEquals("""{"reason":"declined"}""", json(AnalyticsEvent.AddToAccountFailed(AddToAccountFailure.Declined)))
        assertEquals("dev_server_action", AnalyticsEvent.DevServerAction(DevServerUse.Copy).name)
        assertEquals("""{"action":"copy"}""", json(AnalyticsEvent.DevServerAction(DevServerUse.Copy)))
        assertEquals("""{"link":"install_link"}""", json(AnalyticsEvent.LinkOpened(OutboundLink.InstallLink)))
        assertEquals("""{"step":"choose"}""", json(AnalyticsEvent.OnboardingStepViewed(OnboardingStep.Choose)))
        assertEquals("""{"action":"copy_image","from":"menu","outcome":"done"}""",
            json(AnalyticsEvent.ImagePreviewAction(ImageAction.CopyImage, ImageFrom.Menu)))
        assertEquals("""{"action":"zoom","from":"viewer","outcome":"done"}""", json(AnalyticsEvent.ImagePreviewAction(ImageAction.Zoom, ImageFrom.Viewer)))
        // `error` is only there when the action failed, and is one of the catalog's categories.
        for ((error, wire) in listOf(ImageError.Unavailable to "unavailable", ImageError.Convert to "convert", ImageError.Clipboard to "clipboard",
            ImageError.Transfer to "transfer")) {
            assertEquals("""{"action":"download","from":"viewer","outcome":"failed","error":"$wire"}""",
                json(AnalyticsEvent.ImagePreviewAction(ImageAction.Download, ImageFrom.Viewer, error)))
        }
        assertEquals("""{"computers":2,"keys":"moved"}""", json(AnalyticsEvent.SignedOutPairingsMoved(2, MovedKeys.Moved)))
        assertEquals("""{"computers":1,"keys":"rescan"}""", json(AnalyticsEvent.SignedOutPairingsMoved(1, MovedKeys.Rescan)))
        assertEquals("""{"engine":"gemini","reconnects":1,"stuck":"none","failure":"connection","ready_ms":1200}""",
            json(AnalyticsEvent.VoiceCallQuality(VoiceEngineKind.Gemini, CallQuality.Report(1_200, 1, VoiceStuck.None, VoiceFailureKind.Connection))))
        // An engine that was never ready leaves `ready_ms` out.
        assertEquals("""{"engine":"on_device","reconnects":0,"stuck":"connecting","failure":"none"}""",
            json(AnalyticsEvent.VoiceCallQuality(VoiceEngineKind.OnDevice, CallQuality.Report(null, 0, VoiceStuck.Connecting, null))))
    }

    @Test fun `pairing failures map to the catalog's reasons`() {
        assertEquals(PairingFailure.NotPairingLink, PairingFailure.of(PairingProblem(PairingProblem.Kind.NotPairingLink)))
        assertEquals(PairingFailure.UpgradeRequired, PairingFailure.of(HostError(426, "Update")))
        assertEquals(PairingFailure.Expired, PairingFailure.of(HostError(403, "This pairing code expired. Show a new one on the computer.")))
        assertEquals(PairingFailure.Unauthorized, PairingFailure.of(HostError(403, "The computer didn't let this phone in.")))
        assertEquals(PairingFailure.Timeout, PairingFailure.of(IOException("Late", java.net.SocketTimeoutException())))
        assertEquals(PairingFailure.Unreachable, PairingFailure.of(IOException("Offline")))
        assertEquals(PairingFailure.Other, PairingFailure.of(IllegalArgumentException("Use Start over")))
    }

    @Test fun `a refused Boat key is told apart from a save that failed`() {
        assertEquals(CloudKeyOutcome.Refused, CloudKeyOutcome.of(CloudError(422, "keyRefused", "This key has expired. Make a new one.")))
        assertEquals(CloudKeyOutcome.Failed, CloudKeyOutcome.of(CloudError(429, "rateLimited")))
        assertEquals(CloudKeyOutcome.Failed, CloudKeyOutcome.of(IOException("Offline")))
        assertEquals("""{"outcome":"refused","provider":"instabox"}""", JsonObject(AnalyticsEvent.CloudKeySaved(CloudKeyOutcome.Refused, "instabox").properties).toString())
        assertEquals("""{"provider":"instabox"}""", JsonObject(AnalyticsEvent.CloudOfferOpened("instabox").properties).toString())
    }

    @Test fun `a cloud computer the cloud refuses is told apart from one that failed`() {
        val limit = CloudError(409, "conflict", "You can have up to 3 cloud computers. Delete one to make another.")
        assertEquals(CloudCreateOutcome.Refused, CloudCreateOutcome.of(limit))
        assertEquals(CloudCreateOutcome.Refused, CloudCreateOutcome.of(CloudError(422, "keyRefused", "This key has expired. Make a new one.")))
        assertEquals(CloudCreateOutcome.Failed, CloudCreateOutcome.of(CloudError(503, "providerUnavailable")))
        assertEquals(CloudCreateOutcome.Failed, CloudCreateOutcome.of(IOException("Offline")))
        val created = AnalyticsEvent.CloudComputerCreated(CloudCreateOutcome.Created, "boat")
        assertEquals("cloud_computer_created", created.name)
        assertEquals("""{"outcome":"created","provider":"boat"}""", JsonObject(created.properties).toString())
        val renamed = AnalyticsEvent.CloudComputerRenamed(CloudRenameOutcome.Failed)
        assertEquals("cloud_computer_renamed", renamed.name)
        assertEquals("""{"outcome":"failed"}""", JsonObject(renamed.properties).toString())
        val deleted = AnalyticsEvent.CloudComputerDeleted(CloudDeleteOutcome.Deleted, "boat")
        assertEquals("cloud_computer_deleted", deleted.name)
        assertEquals("""{"outcome":"deleted","provider":"boat"}""", JsonObject(deleted.properties).toString())
        val snapshots = AnalyticsEvent.CloudSnapshotsDeleted(CloudDeleteOutcome.Failed, "instabox")
        assertEquals("cloud_snapshots_deleted", snapshots.name)
        assertEquals("""{"outcome":"failed","provider":"instabox"}""", JsonObject(snapshots.properties).toString())
        val removed = AnalyticsEvent.CloudKeyRemoved(CloudRemoveOutcome.Removed, "boat")
        assertEquals("cloud_key_removed", removed.name)
        assertEquals("""{"outcome":"removed","provider":"boat"}""", JsonObject(removed.properties).toString())
        assertEquals(CloudDeleteOutcome.Refused, CloudDeleteOutcome.of(CloudError(409, "conflict", "It's asleep.")))
        assertEquals(CloudDeleteOutcome.Failed, CloudDeleteOutcome.of(CloudError(503, "providerUnavailable")))
        assertEquals(CloudRemoveOutcome.Refused, CloudRemoveOutcome.of(CloudError(409, "conflict")))
        assertEquals(CloudRemoveOutcome.Failed, CloudRemoveOutcome.of(IOException("Offline")))
    }

    @Test fun `a crash keeps its type and Codync frames but never its message`() {
        val error = IllegalStateException("secret prompt text")
        val properties = CrashReports.properties(error).toString()
        assert("IllegalStateException" in properties)
        assert("secret prompt text" !in properties)
    }

    @Test fun `a session ends after half an hour idle or a day in all`() {
        val session = AnalyticsSession()
        val t0 = 1_800_000_000_000L
        val minute = 60_000L
        val first = session.id(t0)
        // A UUIDv7 whose first 48 bits are its start in milliseconds.
        assertEquals("%012x".format(t0) + "7", first.replace("-", "").take(13))
        assertEquals(first, session.id(t0 + 29 * minute))
        val start = t0 + 60 * minute
        val second = session.id(start)
        assert(second != first) { "idle for 31 minutes" }
        // Busy without a break: a new one once it has run for a day.
        var t = start
        while (t <= start + 24 * 60 * minute) { assertEquals(second, session.id(t)); t += 20 * minute }
        assert(session.id(t) != second)
    }
}
