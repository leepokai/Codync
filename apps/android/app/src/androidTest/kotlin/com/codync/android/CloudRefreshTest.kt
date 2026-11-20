package com.codync.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import com.codync.android.core.CloudClient
import com.codync.android.core.CloudError
import com.codync.android.core.DeviceIdentity
import com.codync.android.core.RelayCrypto
import java.io.IOException
import java.util.Base64
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** First-run setup waits on the account's answer, then on a cloud computer started from it. */
class CloudRefreshTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    private fun client(base: String = "https://cloud.invalid", token: suspend () -> String) =
        CloudClient(base, DeviceIdentity(ByteArray(32) { 1 }, ByteArray(32) { 2 }), "2.24.0", token)

    private fun client(failure: IOException) = client { throw failure }

    private fun refresh(accounts: AccountStore, client: CloudClient?) = runBlocking(Dispatchers.Main) {
        withTimeout(10_000) { accounts.state.first { "cloud" !in it.busy } }
        accounts.cloud = client
        accounts.mutableState.update { it.copy(cloudLoaded = false) }
        accounts.refreshCloud()
        withTimeout(10_000) { accounts.state.first { it.cloudLoaded && "cloud" !in it.busy } }
    }

    /** A failed answer ends the wait too, or setup stays on its spinner (iOS `aFailedRefreshEndsSetupsWait`). */
    @Test fun theWaitEndsWithoutACloudAndWhenTheRefreshFails() = compose.withPreviewAccounts { accounts ->
        assertTrue(refresh(accounts, null).cloudComputers.isEmpty())
        // A dropped connection passes quietly.
        assertNull(refresh(accounts, client(IOException("offline"))).error)
        // An answer that stays reaches the dialog.
        val refused = refresh(accounts, client(CloudError(403, "forbidden", "Refused")))
        assertEquals("Refused", refused.error)
        assertTrue(refused.cloudComputers.isEmpty())
    }

    /** A first run waiting on a cloud computer asks the account again after a look the network lost, and the computer
     * again after a request it lost: the cloud still lists that one, and this phone has no code for it. Nobody tapped
     * for those requests, so a failed one raises no dialog (iOS `aWaitForACloudComputerAsksAgainAfterALostLookup`). */
    @Test fun aWaitForACloudComputerAsksAgainAfterALostLookup() = compose.withPreviewAccounts { accounts ->
        val key = ByteArray(32) { 6 }
        val base64 = Base64.getUrlEncoder().withoutPadding()
        val id = RelayCrypto.computerId(key)
        val lookups = AtomicInteger()
        val requests = AtomicInteger()
        MockWebServer().use { server ->
            // The cloud is too busy for the first look and the first request; a request is never approved.
            server.dispatcher = object : Dispatcher() {
                override fun dispatch(request: RecordedRequest): MockResponse {
                    val path = request.path.orEmpty()
                    return when {
                        path.endsWith("/v1/computers") && lookups.incrementAndGet() == 1 -> MockResponse().setResponseCode(503)
                        path.endsWith("/access-requests") && requests.incrementAndGet() == 1 -> MockResponse().setResponseCode(503)
                        path.endsWith("/v1/computers") -> MockResponse().setBody("""{"computers":[{"computerId":"$id","name":"Cloud","signKey":"${base64.encodeToString(key)}","access":"pending","cloud":{"provider":"boat","providerName":"Boat","region":"EU","state":"running"}}]}""")
                        path.endsWith("/access-requests") -> MockResponse().setBody("""{"requestId":"r1","expiresAt":4102444800000}""")
                        path.endsWith("/access-requests/r1") -> MockResponse().setBody("""{"requestId":"r1","status":"pending","hostNonce":"${base64.encodeToString(ByteArray(32) { 9 })}"}""")
                        else -> MockResponse().setBody("{}")
                    }
                }
            }
            val cloud = client(server.url("/").toString()) { "token" }
            runBlocking(Dispatchers.Main) {
                withTimeout(10_000) { accounts.state.first { "cloud" !in it.busy } }
                accounts.cloud = cloud
                val wait = launch { accounts.reachCloudComputer(id, every = 20) }
                try {
                    withTimeout(10_000) { accounts.state.first { id in it.access } }
                    assertNull(accounts.state.value.error)
                } finally {
                    wait.cancelAndJoin()
                    accounts.actions["access:$id"]?.cancelAndJoin()
                }
            }
        }
    }

    /** The wait leaves an error on screen alone, asks again once it's read, and ends when this phone uses the computer
     * (iOS `aWaitForACloudComputerHoldsForAnErrorAndEndsOnceTheComputerIsUsed`). */
    @Test fun aWaitForACloudComputerHoldsForAnErrorAndEndsOnceTheComputerIsUsed() = compose.withPreviewAccounts { accounts ->
        val asked = AtomicInteger()
        runBlocking(Dispatchers.Main) {
            withTimeout(10_000) { accounts.state.first { "cloud" !in it.busy } }
            accounts.cloud = client {
                asked.incrementAndGet()
                throw IOException("offline")
            }
            accounts.mutableState.update { it.copy(error = "Shown") }
            val wait = launch { accounts.reachCloudComputer(previewComputer.id, every = 20) }
            try {
                delay(300)
                assertEquals(0, asked.get())
                assertEquals("Shown", accounts.state.value.error)
                accounts.clearError()
                withTimeout(10_000) { while (asked.get() == 0) delay(20) }
                accounts.open(previewComputer)
                withTimeout(10_000) { wait.join() }
            } finally {
                wait.cancelAndJoin()
                accounts.close(previewComputer.id)
            }
        }
    }
}
