package com.codync.android

import android.appwidget.*
import android.content.ComponentName
import android.net.Uri
import android.graphics.Bitmap
import android.os.Build
import android.os.Bundle
import android.util.AtomicFile
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.core.view.drawToBitmap
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.UUID

/** Opt-in real AppWidgetHost and encrypted fixture queries; never a user's phone. */
class WidgetSystemTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    @Test fun installedWidgetsRenderResizeSelectProvidersAndClearOnAccountChange() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("widgetHost") == "true")
        assumeTrue(Build.PRODUCT.startsWith("sdk"))
        val context = instrumentation.targetContext
        assertNull(PushPreferences.activeUser(context))
        val accounts = accountStore(compose.activity)
        compose.waitUntil(30_000) { !accounts.state.value.loading }
        val computer = LocalStore(context).computers().single()
        requireFixtureHost(computer.id)
        val originalFile = AtomicFile(File(context.noBackupFilesDir, "widget-owner.json"))
        val original = if (originalFile.baseFile.exists()) originalFile.readFully() else null
        val local = LocalStore(context, "widget-host-" + UUID.randomUUID())
        val manager = AppWidgetManager.getInstance(context)
        val host = AppWidgetHost(context, 49371)
        val ids = mutableListOf<Int>()
        var container: FrameLayout? = null
        try {
            local.saveComputers(listOf(computer))
            val owner = WidgetStore.activate(context, local.contextId)
            val usage = UsageReport(listOf(UsageProvider("claude", "Claude", listOf(UsageWindow("session", "Session", 97.0))),
                UsageProvider("codex", "Codex", listOf(UsageWindow("daily", "Daily", 24.0)))))
            WidgetStore.publish(context, owner, listOf(WidgetComputer(computer, (1..6).map { Bot("sample-$it", "Widget bot $it", status = if (it == 1) "needsInput" else "working") }, true)),
                computer, usage, true)
            instrumentation.runOnMainSync {
                container = FrameLayout(compose.activity).apply { setPadding(16, 16, 16, 16) }
                compose.activity.setContentView(container)
                host.startListening()
            }
            for ((receiver, expected) in listOf(BotsWidgetReceiver::class.java to "Widget bot 6",
                TeamWidgetReceiver::class.java to "Widget bot 1", ProviderWidgetReceiver::class.java to "Session")) {
                val id = host.allocateAppWidgetId(); ids.add(id)
                instrumentation.uiAutomation.adoptShellPermissionIdentity(android.Manifest.permission.BIND_APPWIDGET)
                try { assertTrue("Widget binding was refused", manager.bindAppWidgetIdIfAllowed(id, ComponentName(context, receiver))) }
                finally { instrumentation.uiAutomation.dropShellPermissionIdentity() }
                manager.updateAppWidgetOptions(id, Bundle().apply {
                    putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH, 320); putInt(AppWidgetManager.OPTION_APPWIDGET_MAX_WIDTH, 320)
                    putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT, 300); putInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT, 300)
                })
                val info = requireNotNull(manager.getAppWidgetInfo(id))
                instrumentation.runOnMainSync {
                    val view = host.createView(compose.activity, id, info)
                    container!!.removeAllViews()
                    container!!.addView(view, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,
                        (320 * context.resources.displayMetrics.density).toInt()))
                }
                val glance = GlanceAppWidgetManager(context).getGlanceIdBy(id)
                runBlocking { when (receiver) { BotsWidgetReceiver::class.java -> BotsWidget().update(context, glance)
                    TeamWidgetReceiver::class.java -> TeamWidget().update(context, glance); else -> ProviderWidget().update(context, glance) } }
                waitFor(container!!, expected)
                assertTrue(CodyncWidgets.installed(context) >= ids.size)
                if (receiver == ProviderWidgetReceiver::class.java) {
                    WidgetStore.selectProvider(context, id, "codex")
                    runBlocking { ProviderWidget().update(context, glance) }
                    waitFor(container!!, "Daily"); assertTrue(texts(container!!).contains("24%"))
                    manager.updateAppWidgetOptions(id, Bundle().apply {
                        putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH, 160); putInt(AppWidgetManager.OPTION_APPWIDGET_MAX_WIDTH, 160)
                        putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT, 120); putInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT, 120)
                    })
                    runBlocking { ProviderWidget().update(context, glance) }; waitFor(container!!, "Daily")
                }
                snapshot(receiver.simpleName, container!!)
            }
            WidgetStore.retire(context, owner)
            runBlocking { CodyncWidgets.update(context) }
            waitFor(container!!, "Connect a computer in Codync.")
            assertFalse(texts(container!!).contains("Daily"))
        } finally {
            instrumentation.uiAutomation.dropShellPermissionIdentity()
            val active = WidgetStore.owner(context)
            if (active?.context == local.contextId) WidgetStore.retire(context, active)
            ids.forEach(host::deleteAppWidgetId); host.stopListening(); host.deleteHost()
            runBlocking { WidgetRefreshWorker.stop(context) }
            local.erase()
            if (original == null) originalFile.delete() else {
                val output = originalFile.startWrite(); output.write(original); originalFile.finishWrite(output)
            }
            WidgetStore.changes.value++
        }
    }

    @Test fun aBoundedWidgetRefreshReadsTheRealFixtureWithoutChangingItsBots() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("widgetHost") == "true")
        assumeTrue(Build.PRODUCT.startsWith("sdk"))
        val context = instrumentation.targetContext
        assertNull(PushPreferences.activeUser(context))
        val accounts = accountStore(compose.activity)
        val store = compose.computerStore(accounts)
        compose.waitUntil(30_000) { !store.state.value.loading && store.state.value.rosterLoaded }
        requireFixtureHost(LocalStore(context).computers().single().id)
        val started = System.currentTimeMillis()
        val written = runBlocking { WidgetRefreshWorker.refresh(context) }
        val feed = WidgetStore.feed(context)
        // A concurrent foreground report supersedes the slower background read.
        assertTrue(written || feed.botsAt >= started)
        assertEquals("local", feed.scope)
        assertTrue(feed.bots.any { it.name == "Android chat acceptance" })
        assertTrue(feed.botsAt > System.currentTimeMillis() - 30_000)
        val link = requireNotNull(widgetIntent(context, feed, usage = true).data)
        compose.runOnUiThread { accounts.openLink(link) }
        compose.waitUntil(10_000) { accounts.state.value.navigationPage == LinkPage.Usage && store.state.value.selectedBot == null }
        // The usage sheet's title (it refreshes on its own and on pull).
        val refresh = hasText("Usage") and isHeading()
        compose.waitUntil(10_000) { compose.onAllNodes(refresh).fetchSemanticsNodes().size == 1 }
        compose.onNode(refresh).assertIsDisplayed()
        compose.activityRule.scenario.recreate()
        compose.waitUntil(10_000) { compose.onAllNodes(refresh).fetchSemanticsNodes().size == 1 }
        compose.onNode(refresh).assertIsDisplayed()
        val request = accounts.state.value.navigationRequest
        val foreign = Uri.Builder().scheme("codync").authority("usage").appendQueryParameter("scope", "a".repeat(64))
            .appendQueryParameter("computer", feed.computerId).build()
        compose.runOnUiThread { accounts.openLink(foreign) }
        compose.waitUntil(10_000) { accounts.state.value.error?.contains("Switch to the account") == true }
        assertEquals(request, accounts.state.value.navigationRequest)
    }

    private fun texts(root: View): List<String> {
        var values = emptyList<String>()
        instrumentation.runOnMainSync {
            fun visit(view: View): List<String> = buildList {
                if (view is TextView) add(view.text.toString())
                if (view is ViewGroup) for (i in 0 until view.childCount) addAll(visit(view.getChildAt(i)))
            }
            values = visit(root)
        }
        return values
    }
    private fun waitFor(root: View, expected: String) = runBlocking {
        withTimeout(30_000) { while (expected !in texts(root)) delay(100) }
    }
    private fun snapshot(name: String, container: ViewGroup) {
        if (InstrumentationRegistry.getArguments().getString("widgetSnapshots") != "true") return
        instrumentation.waitForIdleSync()
        // Layout follows the RemoteViews text update; wait for the drawn frame too.
        val frame = java.util.concurrent.CountDownLatch(1)
        instrumentation.runOnMainSync { container.postOnAnimation { container.postOnAnimation { frame.countDown() } } }
        assertTrue(frame.await(3, java.util.concurrent.TimeUnit.SECONDS))
        instrumentation.runOnMainSync {
            val bitmap = container.getChildAt(0).drawToBitmap()
            File(instrumentation.targetContext.cacheDir, "widget-$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
    }
}
