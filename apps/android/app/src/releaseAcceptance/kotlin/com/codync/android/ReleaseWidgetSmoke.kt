package com.codync.android

import android.app.Instrumentation
import android.appwidget.AppWidgetHost
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.os.Bundle
import android.util.AtomicFile
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.glance.appwidget.GlanceAppWidgetManager
import com.codync.android.core.*
import java.io.File
import java.util.UUID
import kotlinx.coroutines.*

/** No JUnit/Clerk keep rules: actual minified Glance → Android RemoteViews. */
object ReleaseWidgetSmoke {
    @JvmStatic fun run(instrumentation: Instrumentation) {
        val context = instrumentation.targetContext
        check(PushPreferences.activeUser(context) == null)
        val original = AtomicFile(File(context.noBackupFilesDir, "widget-owner.json"))
        val bytes = if (original.baseFile.exists()) original.readFully() else null
        val computer = LocalStore(context).computers().single()
        check(computer.id == "9ESVKkTXlLyIExxnW7-PIg")
        val local = LocalStore(context, "widget-r8-" + UUID.randomUUID())
        val manager = AppWidgetManager.getInstance(context)
        val host = AppWidgetHost(context, 49372)
        val ids = mutableListOf<Int>()
        try {
            local.saveComputers(listOf(computer))
            val owner = WidgetStore.activate(context, local.contextId)
            WidgetStore.publish(context, owner, computer, listOf(Bot("r8-bot", "R8 bot", status = "needsInput")), UsageReport(listOf(
                UsageProvider("codex", "Codex", listOf(UsageWindow("daily", "Daily", 24.0))))), true, true)
            val widgetId = host.allocateAppWidgetId(); ids.add(widgetId)
            val botId = host.allocateAppWidgetId(); ids.add(botId)
            instrumentation.uiAutomation.adoptShellPermissionIdentity(android.Manifest.permission.BIND_APPWIDGET)
            try {
                check(manager.bindAppWidgetIdIfAllowed(widgetId, ComponentName(context, ProviderWidgetReceiver::class.java)))
                check(manager.bindAppWidgetIdIfAllowed(botId, ComponentName(context, BotsWidgetReceiver::class.java)))
            }
            finally { instrumentation.uiAutomation.dropShellPermissionIdentity() }
            manager.updateAppWidgetOptions(widgetId, Bundle().apply {
                putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH, 300); putInt(AppWidgetManager.OPTION_APPWIDGET_MAX_WIDTH, 300)
                putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT, 160); putInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT, 160)
            })
            WidgetStore.selectProvider(context, widgetId, "codex")
            lateinit var root: FrameLayout
            lateinit var bots: FrameLayout
            instrumentation.runOnMainSync {
                root = FrameLayout(context)
                host.startListening()
                val view = host.createView(context, widgetId, requireNotNull(manager.getAppWidgetInfo(widgetId)))
                root.addView(view)
                bots = FrameLayout(context)
                bots.addView(host.createView(context, botId, requireNotNull(manager.getAppWidgetInfo(botId))))
            }
            val glanceManager = GlanceAppWidgetManager(context)
            val glance = glanceManager.getGlanceIdBy(widgetId)
            val botGlance = glanceManager.getGlanceIdBy(botId)
            runBlocking {
                val providers = glanceManager.getGlanceIds(ProviderWidget::class.java)
                check(glance in providers && botGlance !in providers) { "Minified widget kinds lost their distinct identities" }
                CodyncWidgets.update(context)
            }
            runBlocking { withTimeout(30_000) {
                while (true) {
                    var content = emptyList<String>()
                    var botContent = emptyList<String>()
                    instrumentation.runOnMainSync {
                        fun text(view: View): List<String> = buildList {
                            if (view is TextView) add(view.text.toString())
                            if (view is ViewGroup) for (i in 0 until view.childCount) addAll(text(view.getChildAt(i)))
                        }
                        content = text(root)
                        botContent = text(bots)
                    }
                    if ("Codex" in content && "Daily" in content && "24%" in content && "Bots" in botContent && "R8 bot" in botContent) break
                    delay(100)
                }
            } }
        } finally {
            instrumentation.uiAutomation.dropShellPermissionIdentity()
            val active = WidgetStore.owner(context)
            if (active?.context == local.contextId) WidgetStore.retire(context, active)
            ids.forEach { host.deleteAppWidgetId(it) }; WidgetStore.remove(context, ids.toIntArray())
            host.stopListening(); host.deleteHost()
            runBlocking { WidgetRefreshWorker.stop(context) }
            local.erase()
            if (bytes == null) original.delete() else {
                val output = original.startWrite(); output.write(bytes); original.finishWrite(output)
            }
            WidgetStore.changes.value++
        }
    }
}
