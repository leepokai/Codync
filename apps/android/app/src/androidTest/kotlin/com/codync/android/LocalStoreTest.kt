package com.codync.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.DeviceIdentity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
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

