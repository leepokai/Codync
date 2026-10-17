package com.codync.android

import android.Manifest
import android.accessibilityservice.AccessibilityService
import android.app.Notification
import android.app.NotificationManager
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.os.Build
import android.os.SystemClock
import androidx.core.app.NotificationCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.PushAlert
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import java.io.File
import java.util.UUID

/** Alerts as the push service shows them once their sealed text is open; never touches the saved account. */
class AlertNotificationTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private val manager = context.getSystemService(NotificationManager::class.java)
    private val scope = "alert-test-${UUID.randomUUID()}"
    private val computer = "computer"
    private val posted = mutableListOf<String>()

    private fun face(id: String, name: String, shape: String, color: String, kind: String = "agent", members: List<String> = emptyList()) =
        """{"id":"$id","kind":"$kind","members":${members.joinToString(",", "[", "]") { "\"$it\"" }},"name":"$name","avatarColor":"$color","avatarShape":"$shape"}"""

    private fun show(bot: String, alert: String): Notification {
        val tag = "$scope/$computer/$bot"
        posted += tag
        NotificationDisplay.alert(context, scope, computer, bot, PushAlert.decode(alert))
        val deadline = SystemClock.elapsedRealtime() + 3000
        while (manager.activeNotifications.none { it.tag == tag } && SystemClock.elapsedRealtime() < deadline) SystemClock.sleep(10)
        return manager.activeNotifications.single { it.tag == tag }.notification
    }

    @Before fun allowNotifications() {
        if (Build.VERSION.SDK_INT >= 33 && context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED)
            instrumentation.uiAutomation.grantRuntimePermission(context.packageName, Manifest.permission.POST_NOTIFICATIONS)
    }

    @After fun clear() {
        posted.forEach { manager.cancel(it, 1) }
        ShortcutManagerCompat.removeLongLivedShortcuts(context, posted)
    }

    @Test fun aBotsAlertIsAConversationFromTheBotSayingWhatHappened() {
        val notification = show("reviewer", """{"title":"Reviewer needs you","subtitle":"Response needed","body":"Run cargo test --all?",
            "faces":[${face("reviewer", "Reviewer", "hex", "green")}]}""")
        assertEquals("Response needed", notification.extras.getCharSequence(Notification.EXTRA_SUB_TEXT))
        assertEquals("$scope/$computer/reviewer", notification.shortcutId)
        val style = requireNotNull(NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(notification))
        assertFalse(style.isGroupConversation)
        val message = style.messages.single()
        assertEquals("Reviewer", message.person?.name)
        assertEquals("Run cargo test --all?", message.text)
        assertNotNull(message.person?.icon)
        assertTrue(ShortcutManagerCompat.getDynamicShortcuts(context).any { it.id == "$scope/$computer/reviewer" })
        snapshot("bot")
    }

    @Test fun aGroupsAlertNamesTheGroupAndTheMemberSpeaking() {
        val notification = show("crew", """{"title":"Builder in Crew","subtitle":"Task complete","body":"The release build passed.","from":"builder",
            "faces":[${face("crew", "Crew", "blob", "gray", "group", listOf("reviewer", "builder"))},
            ${face("reviewer", "Reviewer", "hex", "green")},${face("builder", "Builder", "squircle", "orange")}]}""")
        val style = requireNotNull(NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(notification))
        assertTrue(style.isGroupConversation)
        assertEquals("Crew", style.conversationTitle)
        assertEquals("Builder", style.messages.single().person?.name)
        assertEquals("Task complete", notification.extras.getCharSequence(Notification.EXTRA_SUB_TEXT))
        snapshot("group")
    }

    @Test fun anAlertWithoutItsChatsFaceStaysPlain() {
        val notification = show("unknown", """{"title":"Docs","subtitle":"Task failed","body":"The docs build stopped."}""")
        assertNull(NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(notification))
        assertEquals("Docs", notification.extras.getCharSequence(Notification.EXTRA_TITLE))
        assertEquals("Task failed", notification.extras.getCharSequence(Notification.EXTRA_SUB_TEXT))
        assertNull(notification.shortcutId)
        snapshot("plain")
    }

    /** `-e notificationSnapshots true`: the shade with the alert, saved in the app's cache. */
    private fun snapshot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("notificationSnapshots") != "true") return
        val automation = instrumentation.uiAutomation
        automation.performGlobalAction(AccessibilityService.GLOBAL_ACTION_NOTIFICATIONS)
        SystemClock.sleep(3000)
        val shot = requireNotNull(automation.takeScreenshot())
        File(context.cacheDir, "notification-$name.png").outputStream().use { shot.compress(Bitmap.CompressFormat.PNG, 100, it) }
        if (Build.VERSION.SDK_INT >= 31) automation.performGlobalAction(AccessibilityService.GLOBAL_ACTION_DISMISS_NOTIFICATION_SHADE)
        SystemClock.sleep(500)
    }
}
