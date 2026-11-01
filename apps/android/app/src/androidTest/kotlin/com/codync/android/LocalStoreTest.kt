package com.codync.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.Computer
import com.codync.android.core.DeviceIdentity
import com.codync.android.core.RelayCrypto
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class LocalStoreTest {
    @Test fun identitySurvivesRecreationIsScopedAndChangesAfterReset() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val scope = "test-${java.util.UUID.randomUUID()}"
        val store = LocalStore(context, scope)
        try {
            val identity = store.identity()
            assertEquals(identity.publicKey, LocalStore(context, scope).identity().publicKey)
            val other = LocalStore(context, "$scope-other")
            try { assertNotEquals(identity.publicKey, other.identity().publicKey) } finally { other.erase() }
            store.completeSetup()
            store.erase()
            assertFalse(store.setupComplete())
            assertNotEquals(identity.publicKey, store.identity().publicKey)
        } finally { store.erase() }
    }

    /** A computer whose id matches its key, as saved computers must. */
    private fun computer(seed: Int, name: String): Computer {
        val key = DeviceIdentity(ByteArray(32) { seed.toByte() }, ByteArray(32)).publicKey
        return Computer(RelayCrypto.computerId(java.util.Base64.getUrlDecoder().decode(key)), name, key)
    }

    @Test fun aNewAccountTakesTheSignedOutKeysAndComputers() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val scope = "test-${java.util.UUID.randomUUID()}"
        val signedOut = LocalStore(context, "$scope-local")
        val account = LocalStore(context, scope)
        try {
            val identity = signedOut.identity()
            signedOut.saveComputers(listOf(computer(1, "Mac mini")))
            signedOut.saveLastComputer("last")
            assertEquals(LocalStore.Adoption(1, MovedKeys.Moved), account.adopt(signedOut))
            assertEquals(identity.publicKey, account.identity().publicKey)
            assertEquals(listOf("Mac mini"), account.computers().map { it.name })
            assertEquals("last", account.lastComputer())
            assertFalse(signedOut.hasIdentity())
            assertTrue(signedOut.computers().isEmpty())
            assertNull("nothing left to move", account.adopt(signedOut))
        } finally { signedOut.erase(); account.erase() }
    }

    @Test fun anAccountInUseKeepsItsKeyAndMergesTheComputers() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val scope = "test-${java.util.UUID.randomUUID()}"
        val signedOut = LocalStore(context, "$scope-local")
        val account = LocalStore(context, scope)
        try {
            val own = account.identity()
            account.saveComputers(listOf(computer(1, "Account copy")))
            signedOut.identity()
            signedOut.saveComputers(listOf(computer(1, "Signed-out copy"), computer(2, "Linux box")))
            assertEquals(LocalStore.Adoption(2, MovedKeys.Rescan), account.adopt(signedOut))
            assertEquals(own.publicKey, account.identity().publicKey)
            assertEquals(listOf("Account copy", "Linux box"), account.computers().map { it.name })
            assertFalse(signedOut.hasIdentity())
        } finally { signedOut.erase(); account.erase() }
    }

    @Test fun theShortcutsLeadIsSavedPerAccountAndAutomaticIsNone() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val store = LocalStore(context, "test-${java.util.UUID.randomUUID()}")
        try {
            assertNull(store.shortcutLead())
            store.saveShortcutLead("linux")
            assertEquals("linux", store.shortcutLead())
            store.saveShortcutLead(null)
            assertNull(store.shortcutLead())
        } finally { store.erase() }
    }

    @Test fun cryptoOpensTheSharedPushVectorOnAndroidWithoutARegisteredProvider() {
        val assets = InstrumentationRegistry.getInstrumentation().context.assets
        val fixture = assets.open("remote-relay-vectors.json").bufferedReader().use { Json.parseToJsonElement(it.readText()).jsonObject }
        val keys = fixture.getValue("keys").jsonObject
        val push = fixture.getValue("push").jsonObject
        val identity = DeviceIdentity(ByteArray(32) { 3 }, ByteArray(32) { 12 })
        assertEquals(keys.getValue("deviceSignPub").jsonPrimitive.content, identity.publicKey)
        assertEquals(push.getValue("pushPub").jsonPrimitive.content, identity.pushKey)
        val result = identity.openPush(push.getValue("sealed").jsonPrimitive.content, keys.getValue("computerId").jsonPrimitive.content)
        assertEquals(push.getValue("plaintext").jsonPrimitive.content, result.toString(Charsets.UTF_8))
    }
}

