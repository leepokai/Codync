package com.codync.android

import com.codync.android.core.CallQuality
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
        assertEquals("""{"engine":"gemini","reconnects":1,"stuck":"none","failure":"connection","ready_ms":1200}""",
            json(AnalyticsEvent.VoiceCallQuality(VoiceEngineKind.Gemini, CallQuality.Report(1_200, 1, VoiceStuck.None, VoiceFailureKind.Connection))))
        // An engine that was never ready leaves `ready_ms` out.
        assertEquals("""{"engine":"on_device","reconnects":0,"stuck":"connecting","failure":"none"}""",
            json(AnalyticsEvent.VoiceCallQuality(VoiceEngineKind.OnDevice, CallQuality.Report(null, 0, VoiceStuck.Connecting, null))))
    }

    @Test fun `pairing failures map to the catalog's reasons`() {
        assertEquals(PairingFailure.NotPairingLink, PairingFailure.of(PairingProblem(PairingProblem.Kind.NotPairingLink)))
        assertEquals(PairingFailure.UpgradeRequired, PairingFailure.of(HostError(426, "Update")))
        assertEquals(PairingFailure.Expired, PairingFailure.of(HostError(403, "Expired code")))
        assertEquals(PairingFailure.Timeout, PairingFailure.of(IOException("Late", java.net.SocketTimeoutException())))
        assertEquals(PairingFailure.Unreachable, PairingFailure.of(IOException("Offline")))
        assertEquals(PairingFailure.Other, PairingFailure.of(IllegalArgumentException("Use Start over")))
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
