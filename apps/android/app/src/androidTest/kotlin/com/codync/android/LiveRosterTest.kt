package com.codync.android

import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.core.app.ActivityScenario
import androidx.lifecycle.ViewModelProvider
import com.codync.android.core.HostClient
import com.codync.android.core.HostConnector
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Explicit, read-only connection probe. Does not reset identity, accounts, mirrors or host data. */
class LiveRosterTest {
    @Test fun anExistingAccountConnectionReturnsItsCurrentRoster() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        assumeTrue(InstrumentationRegistry.getArguments().getString("inspectHost") == "true")
        val context = instrumentation.targetContext
        val local = LocalStore(context, PushPreferences.activeUser(context) ?: "local")
        assertTrue("An existing identity is required", local.hasIdentity())
        val computer = local.computers().first()
        runBlocking {
            withTimeout(45_000) {
                val identity = local.identity()
                val channel = HostConnector.connect(computer, identity)
                try {
                    HostClient.hello(channel, computer)
                    val roster = HostClient.bots(channel)
                    assertTrue("The selected host must contain a bot for this opt-in probe", roster.isNotEmpty())
                    println("Read-only host roster: ${roster.size} bots via ${channel.route}")
                } finally { channel.close() }
            }
        }
    }

    @Test fun anExistingAccountSyncPopulatesItsLocalMirror() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        assumeTrue(InstrumentationRegistry.getArguments().getString("inspectMirror") == "true")
        val user = PushPreferences.activeUser(instrumentation.targetContext)
        val expectedContext = LocalStore.referenceId(user ?: "local")
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            var model: AppStore? = null
            scenario.onActivity { activity ->
                val store = ViewModelProvider(activity)[AppStore::class.java]
                model = store
                // A locked test phone may stop the Activity while we inspect sync independently.
                store.setForeground(true)
            }
            val store = requireNotNull(model)
            try {
                runBlocking {
                    withTimeout(60_000) {
                        store.state.first { !it.loading && it.contextId == expectedContext && (user == null || it.account.ready) }
                        val synced = store.state.first { it.rosterLoaded && it.bots.isNotEmpty() }
                        println("Account mirror populated: ${synced.bots.size} bots")
                        if (InstrumentationRegistry.getArguments().getString("inspectCatchup") == "true") {
                            val head = requireNotNull(synced.hello).getValue("rev").jsonPrimitive.long
                            val local = LocalStore(instrumentation.targetContext, user ?: "local")
                            MirrorDatabase(instrumentation.targetContext, local.directory, synced.computers.first().id).use { mirror ->
                                while (withContext(Dispatchers.IO) { mirror.revision() } < head) delay(100)
                                println("Account mirror committed catch-up through the host head")
                            }
                        }
                    }
                }
            } finally { instrumentation.runOnMainSync { store.setForeground(false) } }
        }
    }

    @Test fun anExistingDeviceProvidesAPrivatePushAcceptanceDestination() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        assumeTrue(InstrumentationRegistry.getArguments().getString("inspectPush") == "true")
        val context = instrumentation.targetContext
        val local = LocalStore(context, PushPreferences.activeUser(context) ?: "local")
        assertTrue("An existing identity is required", local.hasIdentity())
        val computer = local.computers().first()
        val bot = MirrorDatabase(context, local.directory, computer.id).use { it.snapshot().bots.first() }
        val permitted = context.getSystemService(android.app.NotificationManager::class.java).areNotificationsEnabled()
        assertTrue("Enable notifications in the phone's app settings before physical push acceptance", permitted)
        assertTrue("Enable bot notifications in Codync before push acceptance", computer.notifications)
        val destination = buildJsonObject {
            put("fid", requireNotNull(PushPreferences.fid(context)))
            put("pushKey", local.identity().pushKey)
            put("ctx", local.referenceId)
            put("computerId", computer.id)
            put("botId", bot.id)
        }
        // No seeds, credentials or opaque tickets leave the device. The controller deletes this file after reading.
        File(context.cacheDir, "android-push-acceptance.json").writeText(destination.toString())
    }
}
