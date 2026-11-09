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

    @Test fun `every request names this app and its oldest host, with a bare version`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setBody("""{"computers":[]}"""))
            server.enqueue(MockResponse().setBody("""{"cloudComputers":[]}"""))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit, appVersion = "2.24.0-dev")
            client.computers()
            client.cloudComputers()
            repeat(2) { assertEquals("android/2.24.0; minHost=${VersionMismatch.MIN_HOST}", server.takeRequest().getHeader("Codync-App")) }
        } finally { server.shutdown() }
    }

    @Test fun `account computers say which one Codync Cloud runs, and older clouds say nothing`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setBody("""{"computers":[
                {"computerId":"a","name":"Cloud","signKey":"k","cloud":{"provider":"boat","providerName":"Boat","region":"EU","state":"stopped"}},
                {"computerId":"b","name":"Studio","signKey":"k","cloud":null},{"computerId":"c","name":"Old","signKey":"k"}]}"""))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            val (cloud, plain, old) = client.computers()
            assertEquals(CloudPlacement("boat", "Boat", "EU", "stopped"), cloud.cloud)
            assertTrue(requireNotNull(cloud.cloud).asleep)
            assertNull(plain.cloud)
            assertNull(old.cloud)
        } finally { server.shutdown() }
    }

    @Test fun `the account's cloud computers, created, renamed, preparing and deleted`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setBody("""{"cloudComputers":[]}"""))
            server.enqueue(MockResponse().setBody("""{"cloudComputers":[{"cloudComputerId":"cm_0","name":"Cloud computer","provider":"boat",
                "providerName":"Boat","region":"EU","state":"stopped","computerId":"c0"}],"available":true,"limit":3}"""))
            server.enqueue(MockResponse().setBody("""{"cloudComputer":{"cloudComputerId":"cm_1","name":"Builds","provider":"instabox",
                "providerName":"InstaBox","region":"US","state":"starting","computerId":null,"backupAt":null,"snapshots":false,"createdAt":5}}"""))
            server.enqueue(MockResponse().setBody("""{"cloudComputer":{"cloudComputerId":"cm_1","name":"Nightly","provider":"instabox",
                "providerName":"InstaBox","region":"US","state":"running","computerId":"c1"}}"""))
            server.enqueue(MockResponse().setBody("{}"))
            server.enqueue(MockResponse().setBody("{}"))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            assertEquals(CloudOffer(emptyList(), available = false), client.cloudComputers())
            val offer = client.cloudComputers()
            assertTrue(offer.available)
            assertEquals(3, offer.limit)
            assertEquals(listOf("Cloud computer"), offer.machines.map { it.name })
            assertFalse(offer.machines.single().preparing)
            val created = client.createCloudComputer("instabox", "Builds")
            assertEquals("cm_1", created.cloudComputerId)
            assertEquals("Builds", created.name)
            assertTrue(created.preparing)
            assertEquals(5L, created.createdAt)
            assertFalse(created.snapshots)
            assertTrue(CloudMachine("cm_2", "Cloud computer", "boat", "Boat", "EU", "running").snapshots)
            assertFalse(CloudMachine("cm_2", "Cloud computer", "boat", "Boat", "EU", "failed").preparing)
            assertEquals("Nightly", client.renameCloudComputer("cm_1", "Nightly").name)
            client.deleteCloudSnapshots("cm_1")
            client.deleteCloudComputer("cm_1")
            repeat(2) { assertEquals("GET /v1/cloud-computers", server.takeRequest().requestLine.substringBeforeLast(' ')) }
            val create = server.takeRequest()
            assertEquals("POST /v1/cloud-computers", create.requestLine.substringBeforeLast(' '))
            assertEquals("""{"provider":"instabox","name":"Builds"}""", create.body.readUtf8())
            assertEquals("Bearer session", create.getHeader("Authorization"))
            val rename = server.takeRequest()
            assertEquals("PATCH /v1/cloud-computers/cm_1", rename.requestLine.substringBeforeLast(' '))
            assertEquals("""{"name":"Nightly"}""", rename.body.readUtf8())
            assertEquals("DELETE /v1/cloud-computers/cm_1/snapshots", server.takeRequest().requestLine.substringBeforeLast(' '))
            assertEquals("DELETE /v1/cloud-computers/cm_1", server.takeRequest().requestLine.substringBeforeLast(' '))
        } finally { server.shutdown() }
    }

    @Test fun `a new cloud computer is named after the first free number`() {
        fun named(vararg names: String) = CloudOffer(names.map { CloudMachine("cm_$it", it, "boat", "Boat", "EU", "running") }, true)
        assertEquals("Cloud computer", named().suggestedName())
        assertEquals("Cloud computer", named("Builds").suggestedName())
        assertEquals("Cloud computer 2", named("Cloud computer").suggestedName())
        assertEquals("Cloud computer 3", named("Cloud computer", "Cloud computer 2").suggestedName())
        assertEquals("Cloud computer 2", named("Cloud computer", "Cloud computer 3").suggestedName())
        // Translated, it counts its own words.
        assertEquals("雲端電腦 2", named("雲端電腦", "Cloud computer 2").suggestedName("雲端電腦", "雲端電腦 {n}"))
    }

    @Test fun `a new cloud computer goes where a key works, or where one was picked once it does`() {
        val setup = CloudKeySetup("https://boat.dev/dashboard", "boat env new codync")
        val boat = CloudProvider("boat", "Boat", "EU", "boat_…", setup, CloudCredential("boat_…9abc"))
        val refused = CloudProvider("instabox", "InstaBox", "US", "ibk_…", setup, CloudCredential("ibk_…0001", "This key has expired. Make a new one.", blocking = true))
        val none = CloudProvider("nimbus", "Nimbus", "Asia", "nim_…", setup)
        // Turned down for the plan, not the key: Create tries again.
        val unpaid = boat.copy(credential = CloudCredential("boat_…9abc", "This account needs an active plan."))
        assertEquals(listOf(true, false, false, true), listOf(boat, refused, none, unpaid).map { it.usable })
        val offer = CloudOffer(available = true, providers = listOf(refused, none, boat))
        assertEquals("boat", offer.chosenProvider(null))
        // A key turned down can't be picked; one picked before its key was added is, once it is.
        assertEquals("boat", offer.chosenProvider("instabox"))
        assertEquals("boat", offer.chosenProvider("nimbus"))
        assertEquals("nimbus", offer.withCredential("nimbus", CloudCredential("nim_…0002")).chosenProvider("nimbus"))
        assertNull(offer.copy(providers = listOf(refused, none)).chosenProvider(null))
    }

    @Test fun `a key stays while any cloud computer runs on its provider`() {
        val offer = CloudOffer(listOf("running", "starting").map { CloudMachine("cm_$it", it, "boat", "Boat", "EU", it) }, true)
        assertFalse(offer.canRemoveKey("boat"))
        assertTrue(offer.canRemoveKey("instabox"))
        assertTrue(offer.copy(machines = emptyList()).canRemoveKey("boat"))
    }

    @Test fun `each provider's key is given trimmed, shown by its hint, refused with what to fix and removed`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            val setup = """"setup":{"page":"https://boat.dev/dashboard","commands":"boat env new codync"}"""
            server.enqueue(MockResponse().setBody("""{"cloudComputers":[],"available":true,"providers":[
                {"provider":"boat","name":"Boat","region":"EU","keyPrompt":"boat_…",$setup,
                 "credential":{"hint":"boat_…9abc","problem":"This key has expired. Make a new one.","blocking":true,"updatedAt":5}},
                {"provider":"nimbus","name":"Nimbus","region":"Asia","keyPrompt":"nim_…",$setup,"credential":null,
                 "offer":{"label":"Get a free sandbox","detail":"A free plan.","url":"https://nimbus.example/pricing"}}]}"""))
            server.enqueue(MockResponse().setResponseCode(422).setBody("""{"error":{"code":"keyRefused","message":"That doesn't look like an API key from this provider."}}"""))
            server.enqueue(MockResponse().setBody("""{"credential":{"hint":"boat_…0001","problem":null,"updatedAt":6}}"""))
            server.enqueue(MockResponse().setBody("{}"))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            val offer = client.cloudComputers()
            assertEquals(listOf("Boat", "Nimbus"), offer.providers.map { it.name })
            assertEquals(CloudCredential("boat_…9abc", "This key has expired. Make a new one.", blocking = true), offer.providers[0].credential)
            assertEquals(CloudKeySetup("https://boat.dev/dashboard", "boat env new codync"), offer.providers[0].setup)
            // A free plan only where the cloud names one.
            assertNull(offer.providers[0].offer)
            assertEquals(CloudProviderOffer("Get a free sandbox", "A free plan.", "https://nimbus.example/pricing"), offer.providers[1].offer)
            assertTrue(offer.canRemoveKey("boat"))
            val refused = try { client.saveCloudKey("boat", "sk-1"); error("Expected a refusal") } catch (error: CloudError) { error }
            assertEquals("keyRefused", refused.code)
            assertEquals("That doesn't look like an API key from this provider.", refused.message)
            val saved = client.saveCloudKey("boat", " boat_live_key0001 ")
            assertEquals("boat_…0001", saved.hint)
            assertNull(saved.problem)
            client.deleteCloudKey("boat")
            server.takeRequest()
            server.takeRequest()
            val put = server.takeRequest()
            assertEquals("PUT /v1/cloud-credentials/boat", put.requestLine.substringBeforeLast(' '))
            assertEquals("""{"key":"boat_live_key0001"}""", put.body.readUtf8())
            assertEquals("DELETE /v1/cloud-credentials/boat", server.takeRequest().requestLine.substringBeforeLast(' '))
        } finally { server.shutdown() }
    }

    @Test fun `refusals carry the cloud's own words, and an app floor asks for an update`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setResponseCode(403).setBody("""{"error":{"code":"forbidden","message":"Codync Cloud isn't available for this account yet."}}"""))
            server.enqueue(MockResponse().setResponseCode(409).setBody("""{"error":{"code":"conflict","message":"Your cloud computer is asleep."}}"""))
            server.enqueue(MockResponse().setResponseCode(426).setBody("""{"error":{"code":"appUpdateRequired","message":"Update Codync: security fix."}}"""))
            val client = CloudClient(server.url("/").toString(), identity, { "session" }, OkHttpClient(), 1, Unit)
            suspend fun refusal(call: suspend () -> Unit) = try { call(); error("Expected a refusal") } catch (error: CloudError) { error }
            assertEquals("Codync Cloud isn't available for this account yet.", refusal { client.createCloudComputer("boat", "Cloud computer") }.message)
            assertEquals("Your cloud computer is asleep.", refusal { client.deleteCloudSnapshots("cm_1") }.message)
            val floor = refusal { client.computers() }
            assertEquals("appUpdateRequired", floor.code)
            assertEquals("Update Codync: security fix.", floor.message)
            assertFalse(floor.isTransient)
        } finally { server.shutdown() }
    }

    @Test fun `the relay upgrade names this app and reads an app floor's reason`() = runBlocking {
        val computer = Computer(id, "Cloud", hostKey.base64Url(), cloud = "https://cloud.example")
        val request = HostConnector.relayRequest(computer, identity, null, "2.24.0")
        assertEquals("/v1/relay/device/$id", request.url.encodedPath)
        assertEquals("android/2.24.0; minHost=${VersionMismatch.MIN_HOST}", request.header("Codync-App"))
        assertNotNull(request.header("Codync-Sig"))
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setResponseCode(426).setBody("""{"error":{"code":"appUpdateRequired","message":"Update Codync: security fix."}}"""))
            server.enqueue(MockResponse().setResponseCode(426).setBody("""{"error":{"code":"upgradeRequired","message":"Expected a WebSocket upgrade"}}"""))
            suspend fun refusal() = try { OkHttpChannelSocket.dial(okhttp3.Request.Builder().url(server.url("/v1/relay/device/$id")).build())
                error("Expected a refusal") } catch (error: SocketRefused) { error }
            assertEquals(426 to "Update Codync: security fix.", refusal().let { it.status to it.appFloor })
            assertNull(refusal().appFloor)
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

    @Test fun `signing out deletes only this device from the account, on the token taken before`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setBody("""{"devices":[{"deviceId":"other","deviceKey":"elsewhere"},{"deviceId":"mine","deviceKey":"${identity.publicKey}"}]}"""))
            server.enqueue(MockResponse().setBody("{}"))
            var signedIn = true
            val client = CloudClient(server.url("/").toString(), identity, { check(signedIn); "session" }, OkHttpClient(), 1, Unit)
            val pinned = requireNotNull(client.pinningToken())
            signedIn = false
            pinned.revokeThisDevice()
            val list = server.takeRequest()
            assertEquals("/v1/devices" to "Bearer session", list.path to list.getHeader("Authorization"))
            val delete = server.takeRequest()
            assertEquals("DELETE" to "/v1/devices/mine", delete.method to delete.path)
            assertEquals(null, client.pinningToken())
        } finally { server.shutdown() }
    }

    @Test fun `signing out tells the account this device signed out, on the token taken before`() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setBody("{}"))
            var signedIn = true
            val client = CloudClient(server.url("/").toString(), identity, { check(signedIn); "session" }, OkHttpClient(), 1, Unit)
            val pinned = requireNotNull(client.pinningToken())
            signedIn = false
            pinned.signOutThisDevice()
            val request = server.takeRequest()
            assertEquals(listOf("POST", "/v1/devices/sign-out", "Bearer session"), listOf(request.method, request.path, request.getHeader("Authorization")))
            assertEquals(true, request.getHeader("Codync-Sig") != null)
        } finally { server.shutdown() }
    }
}
