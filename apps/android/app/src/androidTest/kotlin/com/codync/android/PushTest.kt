package com.codync.android

import android.Manifest
import android.app.Notification
import android.app.NotificationManager
import android.content.pm.PackageManager
import android.os.Build
import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.Bot
import com.codync.android.core.Computer
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test
import java.util.UUID

class PushTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext

    private fun waitUntil(condition: () -> Boolean) {
        val deadline = SystemClock.elapsedRealtime() + 3000
        while (!condition() && SystemClock.elapsedRealtime() < deadline) SystemClock.sleep(10)
        assertTrue(condition())
    }

    @Test fun sealedFallbackStaysGenericAndInactiveContextsCannotNotify() {
        val previous = PushPreferences.activeUser(context)
        val previousVisible = PushDelivery.foreground
        val user = "push-test-${UUID.randomUUID()}"
        val local = LocalStore(context, user)
        val fixture = instrumentation.context.assets.open("remote-relay-vectors.json").bufferedReader().use {
            Json.parseToJsonElement(it.readText()).jsonObject.getValue("keys").jsonObject
        }
        val computer = Computer(fixture.getValue("computerId").jsonPrimitive.content, "Fixture", fixture.getValue("hostSignPub").jsonPrimitive.content)
        val manager = context.getSystemService(NotificationManager::class.java)
        val bot = "test-${UUID.randomUUID()}"
        val tag = "${local.referenceId}/${computer.id}/$bot"
        val permission = Build.VERSION.SDK_INT >= 33 && context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        if (permission) instrumentation.uiAutomation.grantRuntimePermission(context.packageName, Manifest.permission.POST_NOTIFICATIONS)
        try {
            local.saveComputers(listOf(computer)); local.identity()
            PushPreferences.active(context, user); PushDelivery.foreground = false
            val data = mapOf("kind" to "alert", "ctx" to local.referenceId, "computerId" to computer.id, "botId" to bot,
                "sealed" to "invalid", "title" to "Never display unsealed private text", "body" to "private content")
            PushService.receive(context, data + ("ctx" to "local"))
            assertTrue(manager.activeNotifications.none { it.tag == tag })
            PushService.receive(context, data)
            waitUntil { manager.activeNotifications.any { it.tag == tag } }
            val notification = manager.activeNotifications.single { it.tag == tag }.notification
            assertEquals(context.getString(R.string.app_name), notification.extras.getString(Notification.EXTRA_TITLE))
            assertEquals("A bot has an update.", notification.extras.getString(Notification.EXTRA_TEXT))
            assertEquals(Notification.VISIBILITY_PRIVATE, notification.visibility)
            manager.cancel(tag, 1)
            waitUntil { manager.activeNotifications.none { it.tag == tag } }
            PushDelivery.foreground = true
            PushService.receive(context, data)
            assertTrue(manager.activeNotifications.none { it.tag == tag })
            PushDelivery.foreground = false
            local.saveComputers(listOf(computer.copy(notifications = false)))
            PushService.receive(context, data)
            assertTrue(manager.activeNotifications.none { it.tag == tag })
        } finally {
            manager.cancel(tag, 1); local.erase()
            PushPreferences.active(context, previous); PushDelivery.foreground = previousVisible
            // Revoking this permission kills the test process on newer Android releases.
            // These storage/identity tests run only on disposable emulator installations.
        }
    }

    @Test fun delegatedTasksRejectStaleWrongGenerationAndPostCompletionUpdates() {
        val local = LocalStore(context, "task-test-${UUID.randomUUID()}")
        try {
            val feed = TaskFeed(local.directory)
            val watch = feed.start(Bot("bot", "Reviewer", rev = 1))
            val now = System.currentTimeMillis() / 1000
            val data = mapOf("taskId" to watch.taskId, "status" to "needsInput", "timestamp" to now.toString(), "staleDate" to (now + 900).toString(), "event" to "update")
            assertNull(feed.remote(data + ("taskId" to "old-delegation"), "bot"))
            assertEquals("needsInput", feed.remote(data, "bot")?.status)
            assertNull(feed.remote(data + ("timestamp" to (now - 1).toString()), "bot"))
            assertNull(feed.remote(data + ("staleDate" to (now - 1).toString()), "bot"))
            assertNull(feed.remote(data + mapOf("event" to "end", "status" to "working"), "bot"))
            val ended = feed.remote(data + mapOf("event" to "end", "status" to "idle"), "bot")
            assertEquals(true, ended?.finished)
            assertNull(feed.remote(data + ("timestamp" to (now + 1).toString()), "bot"))
            val restarted = feed.start(Bot("bot", "Reviewer", rev = 2))
            assertNotEquals(watch.taskId, restarted.taskId)
            assertNull(feed.remote(data, "bot"))
        } finally { local.erase() }
    }

    @Test fun aTaskAlreadyRunningWhenItsSendIsAcknowledgedCanFinishLocally() {
        val local = LocalStore(context, "task-running-test-${UUID.randomUUID()}")
        try {
            val feed = TaskFeed(local.directory)
            val started = feed.start(Bot("bot", "Reviewer", status = "needsInput", rev = 5))
            assertTrue(started.seenRunning)
            assertEquals("needsInput", started.status)
            assertEquals(true, feed.local(Bot("bot", "Reviewer", status = "idle", rev = 6))?.finished)
        } finally { local.erase() }
    }

    @Test fun delegatedTaskMarkersAreBoundedAndAnIdleAcknowledgementCannotFinishThem() {
        val local = LocalStore(context, "task-limit-test-${UUID.randomUUID()}")
        try {
            val feed = TaskFeed(local.directory)
            repeat(24) { feed.start(Bot("bot-$it", "Bot", rev = 1)) }
            assertEquals(16, feed.all().size)
            assertNull(feed.local(Bot("bot-23", "Bot", status = "idle", rev = 2)))
            assertEquals(false, feed.local(Bot("bot-23", "Bot", status = "working", rev = 3))?.finished)
            assertNull(feed.local(Bot("bot-23", "Bot", status = "idle", rev = 2)))
            assertEquals(true, feed.local(Bot("bot-23", "Bot", status = "idle", rev = 4))?.finished)
            feed.clear(); assertTrue(feed.all().isEmpty())
        } finally { local.erase() }
    }
}
