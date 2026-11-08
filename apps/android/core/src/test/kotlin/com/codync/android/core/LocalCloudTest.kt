package com.codync.android.core

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LocalCloudTest {
    @Test fun `plain http counts as a cloud only on this machine and only in debug builds`() {
        try {
            assertFalse(isCloudUrl("http://127.0.0.1:8787"))
            LocalCloud.allowed = true
            assertTrue(isCloudUrl("http://127.0.0.1:8787"))
            assertTrue(isCloudUrl("http://localhost:8787"))
            assertFalse("never another machine", isCloudUrl("http://example.test"))
            assertTrue(isCloudUrl("https://dev-api.codync.dev"))
        } finally {
            LocalCloud.allowed = false
        }
    }
}
