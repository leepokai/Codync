package com.codync.android

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CopyRequestsTest {
    @Test fun `a copy overtaken by a newer one is no longer the one to land`() {
        val requests = CopyRequests()
        val first = requests.start()
        assertTrue(requests.isLatest(first))
        val second = requests.start()
        assertFalse(requests.isLatest(first))
        assertTrue(requests.isLatest(second))
    }
}
