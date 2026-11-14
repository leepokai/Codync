package com.codync.android

import android.content.Context
import java.io.File
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

/**
 * Crashes as `$exception` events for PostHog Error Tracking (iOS `CrashReports`): an uncaught
 * exception is written down as it happens, with its session, and sent on the next launch, with the
 * exception's type and frames but never its message.
 */
internal object CrashReports {
    private const val MAX_FRAMES = 50

    fun start(context: Context) {
        val file = File(context.noBackupFilesDir, "crash-report.json")
        // The previous run's crash, now that analytics is up.
        if (file.exists()) {
            runCatching { com.codync.android.core.WireJson.parseToJsonElement(file.readText()).jsonObject }.getOrNull()
                ?.let { Analytics.record("\$exception", it); Analytics.flush() }
            file.delete()
        }
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            runCatching {
                if (Analytics.enabled) {
                    val session = "\$session_id" to JsonPrimitive(Analytics.sessionId())
                    file.writeText(JsonObject(properties(error) + session).toString())
                }
            }
            previous?.uncaughtException(thread, error)
        }
    }

    /** The `$exception` properties for one crash: type and frames, oldest first and the crashing frame last. */
    internal fun properties(error: Throwable): Map<String, JsonElement> {
        val frames = error.stackTrace.take(MAX_FRAMES).reversed().map { frame ->
            buildJsonObject {
                put("platform", "custom"); put("lang", "kotlin")
                put("function", "${frame.className}.${frame.methodName}")
                // A file name only; a path or anything outside Codync's code stays out.
                frame.fileName?.let { put("filename", it.substringAfterLast('/')) }
                if (frame.lineNumber > 0) put("lineno", frame.lineNumber)
                put("in_app", frame.className.startsWith("com.codync."))
                put("resolved", true)
            }
        }
        val exception = buildJsonObject {
            put("type", error.javaClass.simpleName.ifEmpty { "Crash" })
            put("value", "")
            put("mechanism", buildJsonObject { put("type", "uncaught"); put("handled", false); put("synthetic", false) })
            put("stacktrace", buildJsonObject { put("type", "raw"); put("frames", JsonArray(frames)) })
        }
        return mapOf("\$exception_list" to buildJsonArray { add(exception) }, "\$exception_level" to JsonPrimitive("fatal"),
            "process" to JsonPrimitive("android"), "app_version" to JsonPrimitive(BuildConfig.VERSION_NAME))
    }
}
