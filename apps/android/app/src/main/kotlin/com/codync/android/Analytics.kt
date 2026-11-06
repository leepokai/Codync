package com.codync.android

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.UriHandler
import java.security.MessageDigest
import java.time.Instant
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

/** Reports a screen, sheet or onboarding step each time it appears. */
@Composable internal fun AnalyticsOnAppear(event: AnalyticsEvent) = LaunchedEffect(event) { Analytics.capture(event) }

/** Opens an outbound link, counted by what it leads to (desktop `openLink`). */
internal fun UriHandler.open(url: String, link: OutboundLink) {
    openUri(url)
    Analytics.capture(AnalyticsEvent.LinkOpened(link))
}

/**
 * Product analytics (iOS `Analytics`, docs/features/analytics.md): batches events to PostHog, on
 * unless turned off in Settings. Feature use only, never content. Main thread only.
 */
internal object Analytics {
    private const val ENDPOINT = "https://us.i.posthog.com/batch/"
    private const val API_KEY = "phc_rQHvxdSFKnMquPfMjmiMhXhM4tqJ5qCYsWuVEoFxCCWX"
    private const val BATCH_SIZE = 20
    private const val MAX_PENDING = 500
    private const val FLUSH_DELAY = 30_000L

    /** The signed-in Codync account, read at capture time; null = the install's random ID. */
    var userId: () -> String? = { null }
    private var sharing by mutableStateOf(true)
    /** Settings' switch; absent = on. Observable, so the switch follows it. */
    val enabled: Boolean get() = sharing
    private var prefs: SharedPreferences? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val http by lazy { OkHttpClient() }
    private val pending = mutableListOf<JsonObject>()
    private var timer: Job? = null
    private var sending = false
    private var foreground = false
    private val session = AnalyticsSession()

    fun start(context: Context) {
        if (prefs != null) return
        val stored = context.getSharedPreferences("codync-analytics", Context.MODE_PRIVATE)
        prefs = stored
        sharing = stored.getBoolean("enabled", true)
        CrashReports.start(context)
    }

    /** Turning it off drops everything not yet sent. */
    fun setEnabled(on: Boolean) {
        sharing = on
        prefs?.edit()?.putBoolean("enabled", on)?.apply()
        if (on) return flush()
        pending.clear(); timer?.cancel(); timer = null
    }

    fun capture(event: AnalyticsEvent) = record(event.name, event.properties)

    /** One event for the signed-in account (merging the install ID into it once), else the install ID. */
    fun record(name: String, extra: Map<String, JsonElement>) {
        val stored = prefs ?: return
        if (!enabled) return
        val user = userId()
        if (user != null) {
            val identified = stored.getStringSet("identified", emptySet()).orEmpty()
            if (user !in identified) {
                enqueue("\$identify", user, mapOf("\$anon_distinct_id" to JsonPrimitive(installId(stored))))
                stored.edit().putStringSet("identified", identified + user).apply()
            }
            enqueue(name, user, extra)
        } else enqueue(name, installId(stored), extra)
    }

    /** `onboarding_completed`, once per phone until Start over (iOS: its `onboardingCompleted` default), whichever account. */
    fun completeOnboarding(via: OnboardingVia) {
        val stored = prefs ?: return
        if (stored.getBoolean("onboarded", false)) return
        stored.edit().putBoolean("onboarded", true).apply()
        capture(AnalyticsEvent.OnboardingCompleted(via))
    }

    /** Start over: the next setup is onboarding again. */
    fun forgetOnboarding() { prefs?.edit()?.remove("onboarded")?.apply() }

    /** `app_opened` once per trip to the foreground. */
    fun becameActive(context: Context) { if (!foreground) { foreground = true; capture(AnalyticsEvent.AppOpened.of(context)) } }

    fun enteredBackground() { foreground = false; flush() }

    fun flush() {
        timer?.cancel(); timer = null
        if (sending || pending.isEmpty() || !enabled) return
        val batch = pending.toList()
        val body = buildJsonObject { put("api_key", API_KEY); put("batch", buildJsonArray { batch.forEach { add(it) } }) }.toString()
        sending = true
        scope.launch {
            val ok = withContext(Dispatchers.IO) { post(body) }
            sending = false
            if (ok) pending.removeAll(batch.toSet())
            if (pending.isNotEmpty()) scheduleFlush()
        }
    }

    private fun installId(stored: SharedPreferences): String = stored.getString("install", null)
        ?: UUID.randomUUID().toString().also { stored.edit().putString("install", it).apply() }

    /** The current `$session_id`; a crash written down on another thread takes it along to the next launch. */
    fun sessionId(): String = session.id(System.currentTimeMillis())

    private fun enqueue(name: String, distinctId: String, extra: Map<String, JsonElement>) {
        pending += event(name, distinctId, Instant.now().toString(), mapOf("\$session_id" to JsonPrimitive(sessionId())) + extra)
        if (pending.size > MAX_PENDING) repeat(pending.size - MAX_PENDING) { pending.removeAt(0) }
        if (pending.size >= BATCH_SIZE) flush() else scheduleFlush()
    }

    private fun scheduleFlush() {
        if (timer != null) return
        timer = scope.launch { delay(FLUSH_DELAY); timer = null; flush() }
    }

    /** Sent (or rejected for good) → true; network errors, 429 and 5xx → false, retried next flush. */
    private fun post(body: String): Boolean = try {
        http.newCall(Request.Builder().url(ENDPOINT).post(body.toRequestBody("application/json".toMediaType())).build()).execute()
            .use { it.code < 500 && it.code != 429 }
    } catch (_: java.io.IOException) { false }

    /** One event in PostHog's `/batch/` body, with the properties every event carries. */
    internal fun event(name: String, distinctId: String, timestamp: String, extra: Map<String, JsonElement>): JsonObject = buildJsonObject {
        put("uuid", UUID.randomUUID().toString()); put("event", name); put("distinct_id", distinctId); put("timestamp", timestamp)
        put("properties", JsonObject(common + extra + ("\$process_person_profile" to JsonPrimitive(true))))
    }

    private val common: Map<String, JsonElement> by lazy {
        mapOf("\$lib" to JsonPrimitive("codync-android"), "app_version" to JsonPrimitive(BuildConfig.VERSION_NAME),
            // Codync Dev reports as dev, whatever the build type.
            "env" to JsonPrimitive(if (BuildConfig.APPLICATION_ID.endsWith(".dev")) "dev" else "main"),
            "\$os" to JsonPrimitive("Android"), "\$os_version" to JsonPrimitive(Build.VERSION.RELEASE.orEmpty()))
    }

    /** A computer's analytics key, the same hash its host sends: joins `computer_paired` with that computer's events. */
    fun computerTag(id: String): String =
        MessageDigest.getInstance("SHA-256").digest("codync-analytics:$id".toByteArray()).take(8).joinToString("") { "%02x".format(it) }
}
