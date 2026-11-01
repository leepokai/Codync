package com.codync.android.core

import java.util.concurrent.TimeUnit
import kotlinx.coroutines.*
import kotlinx.serialization.json.*
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.Assert.*
import org.junit.Test

class CloudClientTest {
    private val identity = DeviceIdentity(ByteArray(32) { 3 }, ByteArray(32) { 12 })
    private val hostKey = RelayCrypto.signingPublic(ByteArray(32) { 7 })
    private val id = RelayCrypto.computerId(hostKey)

    @Test fun `account access commits before revealing then pins the compared host key`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            val expiresAt = System.currentTimeMillis() + 60_000
            val hostNonce = ByteArray(32) { 5 }
            server.enqueue(MockResponse().setBody("""{"requestId":"r","expiresAt":$expiresAt}"""))
            server.enqueue(MockResponse().setBody("""{"requestId":"r","computerId":"$id","status":"pending"}"""))
            server.enqueue(MockResponse().setBody("""{"requestId":"r","computerId":"$id","status":"pending","hostNonce":"${hostNonce.base64Url()}"}"""))
            server.enqueue(MockResponse().setBody("{}"))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            val ticket = client.requestAccess(id, hostKey.base64Url())
            assertEquals(hostKey.base64Url(), ticket.signKey)
            val commitRequest = server.takeRequest()
            assertEquals("/v1/computers/$id/access-requests", commitRequest.path)
            assertEquals("Bearer session", commitRequest.getHeader("Authorization"))
            assertTrue(requireNotNull(commitRequest.getHeader("Codync-Sig")).contains("kid=${identity.publicKey}"))
            val commit = Json.parseToJsonElement(commitRequest.body.readUtf8()).jsonObject
            assertEquals(setOf("commit"), commit.keys)
            repeat(2) { assertEquals("/v1/access-requests/r", server.takeRequest().path) }
            val revealRequest = server.takeRequest()
            assertEquals("/v1/access-requests/r/reveal", revealRequest.path)
            val nonce = Json.parseToJsonElement(revealRequest.body.readUtf8()).jsonObject.getValue("nonce").jsonPrimitive.content.decodeBase64Url(32)
            assertEquals(commit.text("commit"), RelayCrypto.sasCommit(identity.deviceKey, nonce).base64Url())
            assertEquals(RelayCrypto.sasCode(hostKey, identity.deviceKey, nonce, hostNonce), ticket.code)
            assertNotEquals(commitRequest.getHeader("Codync-Sig"), revealRequest.getHeader("Codync-Sig"))
        } finally { server.shutdown() }
    }

    @Test fun `account registration proves the Android device key and revocation is never retried`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setResponseCode(403).setBody("""{"error":{"code":"deviceRevoked"}}"""))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            val error = try { client.registerDevice("Samsung"); error("Expected revocation") } catch (error: CloudError) { error }
            assertEquals(403, error.status)
            assertFalse(error.isTransient)
            val request = server.takeRequest()
            assertEquals("/v1/devices", request.path)
            assertEquals("""{"name":"Samsung","platform":"android"}""", request.body.readUtf8())
            assertNotNull(request.getHeader("Codync-Sig"))
            assertEquals(1, server.requestCount)
        } finally { server.shutdown() }
    }

    @Test fun `a pairing code is looked up unsigned and an unknown one fails with the cloud's message`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setBody("""{"link":"codync://pair?v=3"}"""))
            server.enqueue(MockResponse().setResponseCode(404)
                .setBody("""{"error":{"code":"notFound","message":"That code expired or was used. Show a new one on the computer."}}"""))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            assertEquals("codync://pair?v=3", client.pairingLink("QRTX-4821"))
            val request = server.takeRequest()
            assertEquals("GET", request.method)
            assertEquals("/v1/pairing-codes/QRTX-4821", request.path)
            assertEquals("Bearer session", request.getHeader("Authorization"))
            assertNull(request.getHeader("Codync-Sig"))
            val error = try { client.pairingLink("QRTX-4821"); error("Expected a used code") } catch (error: CloudError) { error }
            assertEquals(404, error.status)
            assertEquals("That code expired or was used. Show a new one on the computer.", error.message)
        } finally { server.shutdown() }
    }

    @Test fun `an error page that isn't JSON still fails with its status`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setResponseCode(502).setBody("<html><body>Bad gateway</body></html>"))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            val error = try { client.computers(); error("Expected a cloud error") } catch (error: CloudError) { error }
            assertEquals(502, error.status)
            assertEquals("http502", error.code)
            assertTrue(error.isTransient)
        } finally { server.shutdown() }
    }

    @Test fun `mismatched host identity cannot start an access request`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            try { client.requestAccess("bad", hostKey.base64Url()); error("Expected identity rejection") }
            catch (_: IllegalArgumentException) { }
            assertEquals(0, server.requestCount)
            assertEquals("expired", AccessStatus("r", "futureStatus").state)
        } finally { server.shutdown() }
    }

    @Test fun `cancelling a nonce exchange cancels the HTTP call and withdraws the request`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setBody("""{"requestId":"r","expiresAt":${System.currentTimeMillis() + 60_000}}"""))
            server.enqueue(MockResponse().setBody("""{"requestId":"r","status":"pending"}"""))
            server.enqueue(MockResponse().setBody("{}"))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 60_000, Unit)
            val request = launch { client.requestAccess(id, hostKey.base64Url()) }
            withContext(Dispatchers.IO) { server.takeRequest(5, TimeUnit.SECONDS); server.takeRequest(5, TimeUnit.SECONDS) }
            request.cancelAndJoin()
            val withdrawal = server.takeRequest(5, TimeUnit.SECONDS)
            assertEquals("DELETE", withdrawal?.method)
            assertEquals("/v1/access-requests/r", withdrawal?.path)
        } finally { server.shutdown() }
    }

    @Test fun `revoking access removes only this device's grants on that computer`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setBody("""{"devices":[{"deviceId":"other","deviceKey":"elsewhere","name":"iPhone"},
                {"deviceId":"mine","deviceKey":"${identity.publicKey}","name":"Pixel","revoked":false}]}"""))
            server.enqueue(MockResponse().setBody("""{"grants":[{"grantId":"g1","deviceId":"other"},{"grantId":"g2","deviceId":"mine","scopes":["chat"]}]}"""))
            server.enqueue(MockResponse().setBody("{}"))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            client.revokeAccess(id)
            assertEquals("/v1/devices", server.takeRequest().path)
            assertEquals("/v1/computers/$id/grants", server.takeRequest().path)
            val revoke = server.takeRequest()
            assertEquals("DELETE", revoke.method)
            assertEquals("/v1/computers/$id/grants/g2", revoke.path)
            assertEquals("Bearer session", revoke.getHeader("Authorization"))
            assertEquals(3, server.requestCount)
        } finally { server.shutdown() }
    }

    @Test fun `an unknown device has no grants to revoke`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setBody("""{"devices":[]}"""))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            client.revokeAccess(id)
            assertEquals("/v1/devices", server.takeRequest().path)
        } finally { server.shutdown() }
    }

    private fun certificateBody(exp: Long) =
        """{"certificate":{"payload":${JsonPrimitive("""{"v":1,"deviceKey":"${identity.publicKey}","userId":"u","exp":$exp}""")},"sig":"s"}}"""

    @Test fun `a certificate is reused while more than a day is left and a new key is registered first`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            val day = 24L * 60 * 60 * 1000
            val lasting = DeviceCertificate("""{"exp":${System.currentTimeMillis() + 2 * day}}""", "s")
            assertSame(lasting, client.certificate(lasting, "Pixel"))
            assertEquals(0, server.requestCount)

            val exp = System.currentTimeMillis() + 7 * day
            server.enqueue(MockResponse().setResponseCode(403).setBody("""{"error":{"code":"forbidden"}}"""))
            server.enqueue(MockResponse().setBody("{}"))
            server.enqueue(MockResponse().setBody(certificateBody(exp)))
            val ending = DeviceCertificate("""{"exp":${System.currentTimeMillis() + day / 2}}""", "s")
            val fresh = client.certificate(ending, "Pixel")
            assertEquals(exp, fresh.expiresAt)
            assertEquals("s", fresh.sig)
            val asked = server.takeRequest()
            assertEquals("POST", asked.method)
            assertEquals("/v1/devices/certificate", asked.path)
            assertEquals("Bearer session", asked.getHeader("Authorization"))
            assertTrue(requireNotNull(asked.getHeader("Codync-Sig")).contains("kid=${identity.publicKey}"))
            assertEquals("/v1/devices", server.takeRequest().path)
            assertEquals("/v1/devices/certificate", server.takeRequest().path)
        } finally { server.shutdown() }
    }

    @Test fun `a claim is made then completed with the computer's signed object as is`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setBody("""{"claimId":"clm_1","nonce":"n","expiresAt":1}"""))
            server.enqueue(MockResponse().setBody("""{"computer":{}}"""))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            assertEquals(PairingClaim("clm_1", "n"), client.createClaim())
            val signed = buildJsonObject { put("computerId", id); put("signKey", hostKey.base64Url()); put("sig", "x"); put("device", JsonNull) }
            client.completeClaim("clm_1", signed)
            assertEquals("/v1/claims", server.takeRequest().path)
            val complete = server.takeRequest()
            assertEquals("/v1/claims/clm_1/complete", complete.path)
            assertEquals(signed, Json.parseToJsonElement(complete.body.readUtf8()))
        } finally { server.shutdown() }
    }
}
