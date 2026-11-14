package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ArtifactFileNameTest {
    @Test fun `a shared artifact is its title as one html file of at most 200 bytes`() {
        assertEquals("Weekly report.html", artifactFileName("Weekly report"))
        assertEquals("Q3-Q4 plan.html", artifactFileName("Q3/Q4 plan"))
        val long = artifactFileName("週報".repeat(100))
        assertTrue(long.endsWith(".html") && long.toByteArray().size <= 200)
    }
}
