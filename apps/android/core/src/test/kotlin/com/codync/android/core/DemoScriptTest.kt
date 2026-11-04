package com.codync.android.core

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DemoScriptTest {
    private val now = 1_000_000_000L

    @Test fun `the demo has bots a session with a diff an approval and a routine`() {
        assertEquals(listOf("Scout", "Pacer", "Scribe"), DemoScript.bots(now).map { it.name })
        val entries = DemoScript.entries(now)
        assertTrue("a step with a diff", entries.any { it.kind == "tool" && it.data["diffs"] != null })
        assertTrue("an approval waiting", entries.any { it.kind == "permission" && it.data["status"].toString().contains("pending") })
        assertEquals("Morning check", DemoScript.routines(now).getValue(DemoScript.SCOUT).routines.single().name)
    }

    @Test fun `every entry belongs to a demo bot and the order is stable`() {
        val ids = DemoScript.bots(now).map { it.id }.toSet()
        val entries = DemoScript.entries(now)
        assertTrue(entries.all { it.botId in ids })
        assertEquals(entries.map { it.seq }, entries.map { it.seq }.sorted())
        assertEquals(entries.size, entries.map { it.id }.toSet().size)
    }

    @Test fun `the demo uses the clock it is given`() {
        assertEquals(DemoScript.entries(now), DemoScript.entries(now))
        assertFalse(DemoScript.entries(now) == DemoScript.entries(now + 1))
    }

    @Test fun `the script is made of data only`() {
        // No file, no network, no real computer: the script fails this test if it ever reaches for one.
        val source = File("src/main/kotlin/com/codync/android/core/DemoScript.kt").readText().substringBefore("object DemoScript")
            .plus(File("src/main/kotlin/com/codync/android/core/DemoScript.kt").readText().substringAfter("object DemoScript"))
        for (forbidden in listOf("java.io", "java.net", "OkHttp", "HttpURLConnection", "Socket", "SharedPreferences", "Files.", "System.currentTimeMillis", "Instant.now")) {
            assertFalse("the demo script must not use $forbidden", source.contains(forbidden))
        }
    }
}
