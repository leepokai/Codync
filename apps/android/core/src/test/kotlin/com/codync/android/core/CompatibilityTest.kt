package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class CompatibilityTest {
    @Test fun `each side names the oldest version of the other`() {
        assertNull(VersionMismatch.check("2.17.5-dev", null, "9.0.0"))
        assertEquals(VersionMismatch.UpdateApp("2.18.0"), VersionMismatch.check("2.17.5-dev", "2.18.0", "2.18.0"))
        assertEquals(VersionMismatch.UpdateHost("2.25.0", "2.26.0"), VersionMismatch.check("2.26.0", "2.25.0", "2.3.0"))
        assertNull(VersionMismatch.check("2.26.0", "2.26.0", "2.3.0"))
        assertTrue(AppVersion.isBelow("2.9.3", "2.14.1"))
        assertFalse(AppVersion.isBelow("garbled", "2.14.1"))
    }
}
