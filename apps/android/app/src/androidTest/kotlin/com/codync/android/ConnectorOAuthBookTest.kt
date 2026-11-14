package com.codync.android

import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import java.io.File
import java.util.UUID
import org.junit.Assert.*
import org.junit.Test

class ConnectorOAuthBookTest {
    @Test fun aNewOwnerRestoresOnlyTheStateHashAndOtherAccountsCannotReadIt() {
        val root = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "oauth-fixture-" + UUID.randomUUID()).apply { mkdirs() }
        val first = File(root, "first").apply { mkdirs() }; val other = File(root, "other").apply { mkdirs() }
        try {
            val pending = PendingConnectorOAuth.create(ConnectorSignIn("https://example.com/login?state=fixture-state", "app"), "c", "host", System.currentTimeMillis())
            ConnectorOAuthBook(first).save(pending)
            assertEquals(pending, ConnectorOAuthBook(first).read())
            assertNull(ConnectorOAuthBook(other).read())
            assertFalse(File(first, "connector-oauth.json").readText().contains("fixture-state"))
            ConnectorOAuthBook(first).clear()
            assertNull(ConnectorOAuthBook(first).read())
        } finally { root.deleteRecursively() }
    }
}
