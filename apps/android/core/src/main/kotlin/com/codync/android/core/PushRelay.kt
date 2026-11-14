package com.codync.android.core

import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.serialization.json.*
import okhttp3.*
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.RequestBody.Companion.toRequestBody

object PushRelay {
    private val http = OkHttpClient.Builder().followRedirects(false).followSslRedirects(false).callTimeout(15, TimeUnit.SECONDS).build()

    suspend fun ticket(base: String, token: String, contextId: String, computerId: String, botId: String? = null, taskId: String? = null): String {
        require(isCloudUrl(base)) { "Invalid push relay address" }
        require(token.length in 20..4096 && token.all { it.isLetterOrDigit() || it in "_:.-" }) { "Invalid push token" }
        val body = buildJsonObject {
            put("provider", "fcm"); put("token", token); put("ctx", contextId); put("computerId", computerId)
            put("kind", if (botId == null) "alert" else "liveactivity")
            botId?.let { put("botId", it) }
            taskId?.let { put("taskId", it) }
        }
        val url = base.trimEnd('/').toHttpUrl().newBuilder().addPathSegment("register").build()
        val request = Request.Builder().url(url).post(body.toString().toRequestBody("application/json".toMediaType())).build()
        val call = http.newCall(request)
        return suspendCancellableCoroutine { continuation ->
            continuation.invokeOnCancellation { call.cancel() }
            call.enqueue(object : Callback {
                override fun onFailure(call: Call, e: IOException) { continuation.resumeWith(Result.failure(e)) }
                override fun onResponse(call: Call, response: Response) {
                    try {
                        val ticket = response.use {
                            if (!response.isSuccessful) throw IOException("Push registration failed (${response.code}).")
                            val source = requireNotNull(response.body).source()
                            if (source.request(16_385)) throw IOException("Push response is too large.")
                            val value = Json.parseToJsonElement(source.readUtf8()).jsonObject.getValue("ticket").jsonPrimitive.content
                            require(value.isNotBlank() && value.length <= 8192) { "Invalid push ticket" }
                            value
                        }
                        continuation.resumeWith(Result.success(ticket))
                    } catch (error: Exception) { continuation.resumeWith(Result.failure(error)) }
                }
            })
        }
    }
}
