package com.codync.android

import android.os.Build
import android.util.AtomicFile
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File
import java.util.UUID
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

class WidgetStoreTest {
    @Test fun reconnectingDoesNotFreshenCachedActivityUntilTheMirrorCatchesUp() {
        val cached = AppState(previewComputer, loading = false, rosterLoaded = true, mirrorRevision = 9,
            actualConnection = LinkState.Ready(HostRoute.Direct), hello = buildJsonObject { put("rev", 10) })
        assertFalse(cached.widgetBotsCurrent)
        val caughtUp = cached.copy(mirrorRevision = 10)
        assertTrue(caughtUp.widgetBotsCurrent)
        assertFalse(caughtUp.copy(actualConnection = LinkState.Connecting).widgetBotsCurrent)
        assertFalse(caughtUp.copy(hello = null).widgetBotsCurrent)
        assertFalse(caughtUp.copy(loading = true).widgetBotsCurrent)
    }

    @Test fun accountChangesRejectLateResponsesAndForgettingHidesTheirSavedReport() {
        assumeTrue(Build.PRODUCT.startsWith("sdk"))
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertNull(PushPreferences.activeUser(context))
        val originalFile = AtomicFile(File(context.noBackupFilesDir, "widget-owner.json"))
        val original = if (originalFile.baseFile.exists()) originalFile.readFully() else null
        // The emulator's app may have more than one computer paired; the test needs one of them.
        val computer = LocalStore(context).computers().first()
        val first = LocalStore(context, "widget-test-a-" + UUID.randomUUID())
        val second = LocalStore(context, "widget-test-b-" + UUID.randomUUID())
        try {
            first.saveComputers(listOf(computer)); second.saveComputers(listOf(computer))
            val a = WidgetStore.activate(context, first.contextId)
            assertTrue(WidgetStore.publish(context, a, listOf(WidgetComputer(computer, listOf(Bot("shared", "Account A", lastMessage = "Private chat")), true)), computer, UsageReport(), false))
            val b = WidgetStore.activate(context, second.contextId)
            assertTrue(WidgetStore.publish(context, b, listOf(WidgetComputer(computer, listOf(Bot("shared", "Account B")), true)), computer, UsageReport(), false))
            assertFalse(WidgetStore.publish(context, a, listOf(WidgetComputer(computer, listOf(Bot("shared", "Late Account A")), true)), computer, UsageReport(), false))
            assertEquals("Account B", WidgetStore.feed(context).bots.single().name)
            val revision = WidgetStore.changes.value
            WidgetStore.publish(context, b, listOf(WidgetComputer(computer, listOf(Bot("shared", "Newer Account B")), true)), computer, UsageReport(), false)
            assertFalse(WidgetStore.publish(context, b, listOf(WidgetComputer(computer, listOf(Bot("shared", "Old background B")), true)), computer, UsageReport(), false,
                expectedRevision = revision))
            assertEquals("Newer Account B", WidgetStore.feed(context).bots.single().name)
            val persisted = File(second.directory, "widget-feed.json").readText()
            assertFalse(persisted.contains("Private chat")); assertFalse(persisted.contains("lastMessage"))
            // A second computer: unreachable, it keeps the bots it showed; forgotten here, they go with it.
            val key = ByteArray(32) { 3 }
            val laptop = Computer(RelayCrypto.computerId(key), "Laptop", java.util.Base64.getUrlEncoder().withoutPadding().encodeToString(key))
            second.saveComputers(listOf(computer, laptop))
            WidgetStore.publish(context, b, listOf(WidgetComputer(computer, listOf(Bot("shared", "Newer Account B")), true),
                WidgetComputer(laptop, listOf(Bot("laptop", "Laptop bot")), true)), computer, UsageReport(), false)
            WidgetStore.publish(context, b, listOf(WidgetComputer(computer, emptyList(), true), WidgetComputer(laptop, null, false)), computer, UsageReport(), false)
            assertEquals(listOf("Laptop bot"), WidgetStore.feed(context).bots.map { it.name })
            second.saveComputers(listOf(computer))
            assertTrue(WidgetStore.feed(context).bots.isEmpty())
            second.saveComputers(emptyList())
            assertNull(WidgetStore.feed(context).computerId)
            assertTrue(WidgetStore.feed(context).bots.isEmpty())
            WidgetStore.retire(context, b)
            assertNull(WidgetStore.owner(context))
        } finally {
            val active = WidgetStore.owner(context)
            if (active?.context in setOf(first.contextId, second.contextId)) WidgetStore.retire(context, active)
            first.erase(); second.erase()
            if (original == null) originalFile.delete() else {
                val output = originalFile.startWrite(); output.write(original); originalFile.finishWrite(output)
            }
            WidgetStore.changes.value++
        }
    }
}
