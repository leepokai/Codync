package com.codync.android

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.work.*
import com.codync.android.core.*
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.installations.FirebaseInstallations
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.*
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.flow.first
import kotlinx.serialization.json.*

/** App-owned preferences are excluded from backup with the rest of the application. */
object PushPreferences {
    private fun prefs(context: Context) = context.getSharedPreferences("codync-push", Context.MODE_PRIVATE)
    fun activeUser(context: Context): String? = prefs(context).getString("user", null)
    fun tasks(context: Context) = prefs(context).getBoolean("tasks", true)
    fun asked(context: Context) = prefs(context).getBoolean("asked", false)
    fun markAsked(context: Context) { prefs(context).edit().putBoolean("asked", true).apply() }
    fun fid(context: Context) = prefs(context).getString("fid", null)
    fun fid(context: Context, token: String) { prefs(context).edit().putString("fid", token).apply() }
    fun active(context: Context, user: String?) = synchronized(this) { prefs(context).edit().putString("user", user).putBoolean("retired", false).commit() }
    fun retired(context: Context) = prefs(context).getBoolean("retired", false)
    fun retire(context: Context) = synchronized(this) { prefs(context).edit().putBoolean("retired", true).commit() }
    fun tasks(context: Context, enabled: Boolean) { prefs(context).edit().putBoolean("tasks", enabled).commit() }
    /** 2.28.0 had one push switch for every computer; switched off, it becomes each saved computer's own, once (iOS `splitPushSwitch`). */
    fun splitSwitch(context: Context) {
        synchronized(this) {
            val prefs = prefs(context)
            if (!prefs.contains("enabled")) return
            if (!prefs.getBoolean("enabled", true)) LocalStore.pushOffEverywhere(context)
            prefs.edit().remove("enabled").commit()
        }
    }
    fun reset(context: Context) {
        val token = fid(context)
        prefs(context).edit().clear().putString("fid", token).commit()
    }
}

object PushDelivery {
    private val gate = Mutex()
    @Volatile var foreground = false

    suspend fun retire(context: Context) {
        PushPreferences.retire(context)
        WorkManager.getInstance(context).cancelUniqueWork(WORK_NAME)
        gate.withLock {
            context.getSystemService(NotificationManager::class.java).cancelAll()
            AlertConversations.forget(context)
        }
    }
    suspend fun activate(context: Context, user: String?) {
        WorkManager.getInstance(context).cancelUniqueWork(WORK_NAME)
        gate.withLock { PushPreferences.active(context, user) }
    }

    fun schedule(context: Context) {
        if (!BuildConfig.PUSH_CONFIGURED) return
        val request = OneTimeWorkRequestBuilder<PushRegistrationWorker>()
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 15, TimeUnit.SECONDS).build()
        WorkManager.getInstance(context).enqueueUniqueWork(WORK_NAME, ExistingWorkPolicy.REPLACE, request)
    }

    suspend fun sync(context: Context, local: LocalStore, computer: Computer, active: EncryptedChannel) = gate.withLock {
        syncLocked(context, local, computer, active)
    }

    private suspend fun syncLocked(context: Context, local: LocalStore, computer: Computer, active: EncryptedChannel) {
        if (!BuildConfig.PUSH_CONFIGURED) return
        if (PushPreferences.retired(context) || local.contextId != (PushPreferences.activeUser(context) ?: "local")) return
        val permitted = context.getSystemService(NotificationManager::class.java).areNotificationsEnabled()
        val feed = TaskFeed(local.directory, computer.id)
        if (!PushPreferences.tasks(context) || !permitted) {
            feed.all().forEach { active.call("unregisterActivity", body("botId" to it.botId)) }
            feed.clear()
        }
        if (!computer.notifications || !permitted) {
            active.call("unregisterDevice")
            if (!permitted || !PushPreferences.tasks(context) || feed.all().none { !it.finished }) return
        }
        val token = PushPreferences.fid(context) ?: suspendCancellableCoroutine<String> { continuation ->
            FirebaseMessaging.getInstance().register().addOnSuccessListener {
                FirebaseInstallations.getInstance().id.addOnSuccessListener { value ->
                    PushPreferences.fid(context, value)
                    continuation.resumeWith(Result.success(value))
                }.addOnFailureListener { error -> continuation.resumeWith(Result.failure(error)) }
            }.addOnFailureListener { error -> continuation.resumeWith(Result.failure(error)) }
        }
        val identity = withContext(Dispatchers.IO) { local.identity() }
        if (computer.notifications) {
            val ticket = PushRelay.ticket(BuildConfig.PUSH_RELAY_URL, token, local.referenceId, computer.id)
            currentCoroutineContext().ensureActive()
            active.call("registerDevice", buildJsonObject {
                put("ticket", ticket); put("relay", BuildConfig.PUSH_RELAY_URL); put("name", Build.MODEL.take(100))
                put("pushKey", identity.pushKey); put("ctx", local.referenceId)
            })
        }
        if (PushPreferences.tasks(context)) for (watch in feed.all().filter { !it.finished }) {
            val activity = PushRelay.ticket(BuildConfig.PUSH_RELAY_URL, token, local.referenceId, computer.id, watch.botId, watch.taskId)
            currentCoroutineContext().ensureActive()
            active.call("registerActivity", body("botId" to watch.botId, "ticket" to activity))
        }
    }

    suspend fun background(context: Context) = gate.withLock {
        if (!BuildConfig.PUSH_CONFIGURED) return@withLock
        if (PushPreferences.retired(context)) return@withLock
        withContext(Dispatchers.IO) { PushPreferences.splitSwitch(context) }
        val local = LocalStore(context, PushPreferences.activeUser(context) ?: "local")
        val computers = withContext(Dispatchers.IO) { local.computers() }
        if (computers.isEmpty() || !local.hasIdentity()) return@withLock
        // Every computer gets its turn; the first failure is reported once all were tried.
        var failure: Exception? = null
        for (computer in computers) {
            var active: EncryptedChannel? = null
            try {
                withTimeout(45_000) {
                    val identity = withContext(Dispatchers.IO) { local.identity() }
                    val connected = HostConnector.connect(computer, identity)
                    active = connected
                    connected.state.firstReady()
                    syncLocked(context, local, computer, connected)
                }
            } catch (error: Exception) {
                if (error is CancellationException && error !is TimeoutCancellationException) throw error
                failure = failure ?: error
            } finally { active?.close() }
        }
        failure?.let { throw it }
    }

    const val WORK_NAME = "codync-push-registration"
}

class PushRegistrationWorker(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result {
        if (PushPreferences.retired(applicationContext)) return Result.success()
        try {
            PushDelivery.background(applicationContext)
            return Result.success()
        } catch (error: CancellationException) {
            if (error !is TimeoutCancellationException) throw error
            return if (runAttemptCount < 8) Result.retry() else Result.failure()
        } catch (error: HostError) {
            return if (error.status in listOf(403, 426)) Result.failure() else Result.retry()
        } catch (_: Exception) { return if (runAttemptCount < 8) Result.retry() else Result.failure() }
    }
}

private suspend fun kotlinx.coroutines.flow.StateFlow<LinkState>.firstReady() {
    val result = first { it is LinkState.Ready || it is LinkState.ComputerOffline || it is LinkState.Failed }
    if (result !is LinkState.Ready) throw java.io.IOException("Computer unavailable")
}

class PushService : FirebaseMessagingService() {
    override fun onRegistered(installationId: String) {
        val changed = PushPreferences.fid(this) != installationId
        PushPreferences.fid(this, installationId)
        if (changed) PushDelivery.schedule(this)
    }
    override fun onMessageReceived(message: RemoteMessage) { receive(this, message.data) }

    companion object {
        /** Local, bounded work only: notification text is never fetched from the network. */
        fun receive(context: Context, data: Map<String, String>) {
            synchronized(PushPreferences) {
            try {
                if (PushPreferences.retired(context)) return
                PushPreferences.splitSwitch(context)
                val user = PushPreferences.activeUser(context)
                val scope = LocalStore.referenceId(user ?: "local")
                if (data["ctx"] != scope || data["kind"] !in setOf("alert", "task")) return
                val local = LocalStore(context, user ?: "local")
                val computerId = data["computerId"] ?: return
                val computer = local.computers().firstOrNull { it.id == computerId } ?: return
                val botId = data["botId"]?.takeIf { it.matches(Regex("[A-Za-z0-9_-]{1,128}")) } ?: return
                if (!context.getSystemService(NotificationManager::class.java).areNotificationsEnabled()) return
                NotificationDisplay.channels(context)
                if (data["kind"] == "task") {
                    if (!PushPreferences.tasks(context)) return
                    val watch = TaskFeed(local.directory, computerId).remote(data, botId) ?: return
                    NotificationDisplay.task(context, scope, computerId, watch)
                    return
                }
                if (!computer.notifications || PushDelivery.foreground) return
                var alert = PushAlert(title = context.getString(R.string.app_name), body = "A bot has an update.")
                try {
                    if (local.hasIdentity()) {
                        val sealed = requireNotNull(data["sealed"]).takeIf { it.length <= 8192 } ?: return
                        alert = PushAlert.decode(local.identity().openPush(sealed, computerId).toString(Charsets.UTF_8))
                    }
                } catch (_: Exception) { /* Locked/unavailable key: show only a generic fallback. */ }
                NotificationDisplay.alert(context, scope, computerId, botId, alert, data["category"])
            } catch (_: Exception) { /* A malformed push cannot crash the Firebase service. */ }
            }
        }
    }
}

internal object NotificationDisplay {
    const val ALERTS = "codync-alerts"
    const val TASKS = "codync-tasks"
    /** The tapped alert's kind (the host's `AlertKind`), for `notification_opened`. */
    const val ALERT_KIND = "codync:alertKind"
    fun channels(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(ALERTS, "Bot updates", NotificationManager.IMPORTANCE_HIGH))
        manager.createNotificationChannel(NotificationChannel(TASKS, "Delegated tasks", NotificationManager.IMPORTANCE_LOW))
    }
    private fun open(context: Context, scope: String, computer: String, bot: String): Intent {
        val url = Uri.Builder().scheme(BuildConfig.URL_SCHEME).authority("bot").appendPath(bot).appendQueryParameter("scope", scope).appendQueryParameter("computer", computer).build()
        return Intent(context, MainActivity::class.java).setAction(Intent.ACTION_VIEW).setData(url)
    }
    /** An alert's tap carries its kind; its own request code keeps it apart from the same bot's task. */
    private fun builder(context: Context, scope: String, computer: String, bot: String, channel: String, alertKind: String? = null): NotificationCompat.Builder {
        val intent = open(context, scope, computer, bot).apply { alertKind?.let { putExtra(ALERT_KIND, it) } }
        val pending = PendingIntent.getActivity(context, if (alertKind != null) 1 else 0, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val generic = NotificationCompat.Builder(context, channel).setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(context.getString(R.string.app_name)).setContentText("A bot has an update.").build()
        return NotificationCompat.Builder(context, channel).setSmallIcon(R.drawable.ic_notification).setContentIntent(pending)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE).setPublicVersion(generic).setGroup("$scope/$computer")
    }
    /** The alert's title, what happened and its text; with the chat's face, a conversation from the bot. */
    fun alert(context: Context, scope: String, computer: String, bot: String, alert: PushAlert, kind: String?) {
        channels(context)
        val tag = "$scope/$computer/$bot"
        val builder = builder(context, scope, computer, bot, ALERTS, kind.orEmpty()).setContentTitle(alert.title).setContentText(alert.body)
            .setSubText(alert.subtitle).setStyle(NotificationCompat.BigTextStyle().bigText(alert.body))
            .setAutoCancel(true).setCategory(NotificationCompat.CATEGORY_MESSAGE)
        alert.copy(faces = alert.faces.withCustomIcons(notificationIcons(context, scope, computer))).conversation(bot)?.let { conversation ->
            // A system that refuses the conversation still shows the plain alert.
            try { AlertConversations.style(context, builder, tag, computer, open(context, scope, computer, bot), alert.body, conversation) }
            catch (error: Exception) { android.util.Log.w("Codync", "Alert shown without its conversation", error) }
        }
        try { context.getSystemService(NotificationManager::class.java).notify(tag, 1, builder.build()) } catch (_: SecurityException) { }
    }
    /**
     * A delegated task: on Android 16 a Live update (promoted ongoing notification) with its
     * progress, running time and a status bar chip; before that, an ongoing notification.
     */
    fun task(context: Context, scope: String, computer: String, watch: TaskWatch) {
        channels(context)
        val text = when (watch.status) { "working" -> watch.activity.ifEmpty { "Working…" }; "needsInput" -> "Needs your approval"
            "error" -> "Could not finish"; else -> "Done" }
        val tag = "$scope/$computer/${watch.botId}"
        val manager = context.getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= 36) {
            try { manager.notify(tag, 2, liveUpdate(context, scope, computer, watch, text)); return }
            catch (_: SecurityException) { return }
            // A system that rejects the Live update still gets the plain ongoing notification below.
            catch (error: IllegalArgumentException) { android.util.Log.w("Codync", "Live update rejected", error) }
        }
        val notification = builder(context, scope, computer, watch.botId, TASKS).setContentTitle(watch.name).setContentText(text)
            .setOngoing(!watch.finished).setOnlyAlertOnce(true).setAutoCancel(watch.finished)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS).setTimeoutAfter(if (watch.finished) 60_000 else maxOf(1, watch.deadline - System.currentTimeMillis())).build()
        try { manager.notify(tag, 2, notification) } catch (_: SecurityException) { }
    }

    /** The Live update: four segments filling with the task's phase, in its color; the chip counts up. */
    @androidx.annotation.RequiresApi(36)
    private fun liveUpdate(context: Context, scope: String, computer: String, watch: TaskWatch, text: String): Notification {
        val needs = watch.status == "needsInput"
        // The phase's color, as the iOS Live Activity tints it: ink while working, amber, red, green.
        val tint = when (watch.status) { "needsInput" -> 0xFFE0A030.toInt(); "error" -> 0xFFD0453A.toInt(); "idle" -> 0xFF3E9B45.toInt()
            else -> 0xFF5F6368.toInt() }
        val filled = when (watch.status) { "working" -> 2; "needsInput" -> 3; else -> 4 }
        // Styled by progress: segments past the point fade on their own, in every theme.
        val style = Notification.ProgressStyle().setStyledByProgress(true)
            .setProgressSegments(List(4) { Notification.ProgressStyle.Segment(25).setColor(tint) })
            .setProgress(filled * 25)
        val custom = notificationIcons(context, scope, computer)["bot:${watch.botId}"]
        val face = notificationFace(watch.avatarShape, watch.avatarColor, custom)
        val open = builder(context, scope, computer, watch.botId, TASKS).build().contentIntent
        val notification = Notification.Builder(context, TASKS).setSmallIcon(R.drawable.ic_notification).setContentTitle(watch.name).setContentText(text)
            .setLargeIcon(android.graphics.drawable.Icon.createWithBitmap(face)).setContentIntent(open).setStyle(style)
            .setOngoing(!watch.finished).setOnlyAlertOnce(true).setAutoCancel(watch.finished).setCategory(Notification.CATEGORY_PROGRESS)
            .setWhen(watch.startedAt).setShowWhen(true).setUsesChronometer(!watch.finished)
            .setTimeoutAfter(if (watch.finished) 60_000 else maxOf(1, watch.deadline - System.currentTimeMillis()))
            .apply { if (needs) addAction(Notification.Action.Builder(null, "Review", open).build()) }
        // Promotion (the Lock Screen Live update and the status bar chip) arrived in Android 16 QPR1.
        if (LiveUpdates.supported) notification.setRequestPromotedOngoing(!watch.finished)
            .setShortCriticalText(when (watch.status) { "needsInput" -> "Needs you"; "error" -> "Failed"; "idle" -> "Done"; else -> null })
        return notification.build()
    }

    /** Notifications of one kind ([id]); with [prefix], only those whose tag starts with it (one computer's). */
    fun cancel(context: Context, id: Int, prefix: String = "") {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.activeNotifications.filter { it.id == id && it.tag.orEmpty().startsWith(prefix) }.forEach { manager.cancel(it.tag, it.id) }
    }
}
