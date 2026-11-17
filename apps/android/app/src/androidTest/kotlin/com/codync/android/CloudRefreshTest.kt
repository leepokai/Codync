package com.codync.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import com.codync.android.core.CloudClient
import com.codync.android.core.CloudError
import com.codync.android.core.DeviceIdentity
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** First-run setup waits on the account's answer: a failed one ends the wait too, or setup stays on its spinner (iOS `aFailedRefreshEndsSetupsWait`). */
class CloudRefreshTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    private fun client(failure: IOException) =
        CloudClient("https://cloud.invalid", DeviceIdentity(ByteArray(32) { 1 }, ByteArray(32) { 2 }), "2.24.0") { throw failure }

    private fun refresh(accounts: AccountStore, client: CloudClient?) = runBlocking(Dispatchers.Main) {
        withTimeout(10_000) { accounts.state.first { "cloud" !in it.busy } }
        accounts.cloud = client
        accounts.mutableState.update { it.copy(cloudLoaded = false) }
        accounts.refreshCloud()
        withTimeout(10_000) { accounts.state.first { it.cloudLoaded && "cloud" !in it.busy } }
    }

    @Test fun theWaitEndsWithoutACloudAndWhenTheRefreshFails() = compose.withPreviewAccounts { accounts ->
        assertTrue(refresh(accounts, null).cloudComputers.isEmpty())
        // A dropped connection passes quietly.
        assertNull(refresh(accounts, client(IOException("offline"))).error)
        // An answer that stays reaches the dialog.
        val refused = refresh(accounts, client(CloudError(403, "forbidden", "Refused")))
        assertEquals("Refused", refused.error)
        assertTrue(refused.cloudComputers.isEmpty())
    }
}
