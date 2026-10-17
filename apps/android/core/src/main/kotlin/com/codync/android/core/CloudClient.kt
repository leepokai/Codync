package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.*
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import java.util.concurrent.TimeUnit

@Serializable
data class CloudComputer(val computerId: String, val name: String, val signKey: String,
    val platform: String? = null, val device: String? = null, val boxKey: String? = null,
    val version: String? = null, val online: Boolean? = null, val lastSeenAt: Long? = null,
    val claimedAt: Long? = null, val access: String? = null)

data class AccessTicket(val requestId: String, val expiresAt: Long, val code: String, val computerId: String, val signKey: String)

@Serializable
data class AccessStatus(val requestId: String, val status: String, val computerId: String? = null,
    val expiresAt: Long? = null, val hostNonce: String? = null, val grantId: String? = null) {
    // Unrecognized terminal states never authorize the device.
    val state: String get() = status.takeIf { it in setOf("pending", "approved", "denied", "expired", "cancelled") } ?: "expired"
}

class CloudError(val status: Int, val code: String, detail: String? = null) : IOException(when (code) {
    "requestExpired" -> "The request expired. Ask again."
    "accountDeleted" -> "This account was deleted."
    "rateLimited" -> "Too many requests. Try again in a little while."
    else -> detail ?: "Codync cloud error ($code)"
}) {
    val isTransient: Boolean get() = status >= 500 || status == 429 || status == 401
}

/** Existing /v1 account protocol; a login is never a host authorization. */
class CloudClient private constructor(base: String, private val identity: DeviceIdentity,
    private val token: suspend () -> String, private val http: OkHttpClient,
    private val pollInterval: Long) {
    private val baseUrl = base.trimEnd('/').toHttpUrl()
    private val json = Json { ignoreUnknownKeys = true }

    constructor(base: String, identity: DeviceIdentity, token: suspend () -> String) : this(base, identity, token,
        OkHttpClient.Builder().followRedirects(false).followSslRedirects(false).callTimeout(20, TimeUnit.SECONDS).build(), 2_000) {
        require(isCloudUrl(base)) { "Invalid cloud address" }
    }

    internal constructor(base: String, identity: DeviceIdentity, token: suspend () -> String,
        http: OkHttpClient, pollInterval: Long, testOnly: Unit) : this(base, identity, token, http, pollInterval)

    suspend fun registerDevice(name: String) { request("POST", listOf("v1", "devices"),
        buildJsonObject { put("name", name); put("platform", "android") }, signed = true) }

    suspend fun computers(): List<CloudComputer> {
        val result = request("GET", listOf("v1", "computers"), signed = true)
        return json.decodeFromJsonElement(result.getValue("computers"))
    }

    suspend fun requestAccess(id: String, signKey: String): AccessTicket {
        val hostKey = signKey.decodeBase64Url(32)
        require(RelayCrypto.computerId(hostKey) == id) { "This computer's key doesn't match its ID." }
        val nonce = RelayCrypto.randomBytes(32)
        var requestId: String? = null
        try {
            val created = request("POST", listOf("v1", "computers", id, "access-requests"),
                buildJsonObject { put("commit", RelayCrypto.sasCommit(identity.deviceKey, nonce).base64Url()) }, signed = true)
            val rid = created.getValue("requestId").jsonPrimitive.content
            requestId = rid
            val expiresAt = created.getValue("expiresAt").jsonPrimitive.long
            var hostNonce: ByteArray? = null
            while (hostNonce == null) {
                if (System.currentTimeMillis() >= expiresAt) throw CloudError(410, "requestExpired")
                val status = accessStatus(rid)
                require(status.requestId == rid && (status.computerId == null || status.computerId == id)) { "Invalid access response" }
                if (status.state != "pending") throw CloudError(0, status.state, "The computer didn't take the request.")
                hostNonce = status.hostNonce?.decodeBase64Url(32)
                if (hostNonce == null) delay(pollInterval)
            }
            request("POST", listOf("v1", "access-requests", rid, "reveal"),
                buildJsonObject { put("nonce", nonce.base64Url()) }, signed = true)
            return AccessTicket(rid, expiresAt, RelayCrypto.sasCode(hostKey, identity.deviceKey, nonce, hostNonce), id, signKey)
        } catch (error: Throwable) {
            // A cancelled nonce exchange leaves no invisible request waiting at the host.
            requestId?.let { rid -> withContext(NonCancellable) { try { cancelAccess(rid) } catch (_: Exception) { } } }
            throw error
        } finally { nonce.fill(0) }
    }

    /** Takes back this device's grants on a computer, including one approved for an earlier install. */
    suspend fun revokeAccess(computerId: String) {
        val devices = request("GET", listOf("v1", "devices")).getValue("devices").jsonArray.map { it.jsonObject }
        val device = devices.firstOrNull { it.text("deviceKey") == identity.publicKey }?.text("deviceId") ?: return
        val grants = request("GET", listOf("v1", "computers", computerId, "grants")).getValue("grants").jsonArray.map { it.jsonObject }
        for (grant in grants.filter { it.text("deviceId") == device }) {
            request("DELETE", listOf("v1", "computers", computerId, "grants", requireNotNull(grant.text("grantId")) { "Invalid grant" }))
        }
    }

    /** Takes a computer out of the account; its grants and pending requests go with it. */
    suspend fun removeComputer(id: String) { request("DELETE", listOf("v1", "computers", id)) }

    suspend fun accessStatus(id: String): AccessStatus {
        val result = request("GET", listOf("v1", "access-requests", id))
        return json.decodeFromJsonElement(result)
    }
    suspend fun cancelAccess(id: String) { request("DELETE", listOf("v1", "access-requests", id)) }

    private suspend fun request(method: String, path: List<String>, body: JsonObject? = null, signed: Boolean = false): JsonObject {
        val url = baseUrl.newBuilder().apply { path.forEach(::addPathSegment) }.build()
        val bytes = body?.toString()?.toByteArray() ?: byteArrayOf()
        val jwt = token()
        val builder = Request.Builder().url(url).header("Authorization", "Bearer $jwt")
            .method(method, if (body != null) bytes.toRequestBody("application/json".toMediaType()) else null)
        if (signed) builder.header("Codync-Sig", RequestSigner.header(identity, method, url, bytes))
        val call = http.newCall(builder.build())
        val value = suspendCancellableCoroutine<JsonObject> { continuation ->
            continuation.invokeOnCancellation { call.cancel() }
            call.enqueue(object : Callback {
                override fun onFailure(call: Call, e: IOException) { continuation.resumeWith(Result.failure(e)) }
                override fun onResponse(call: Call, response: Response) {
                    try {
                        val parsed = response.use {
                            val source = response.body?.source()
                            if (source?.request(MAX_RESPONSE + 1) == true) throw IOException("Cloud response is too large.")
                            val text = source?.readUtf8().orEmpty().ifBlank { "{}" }
                            val result = json.parseToJsonElement(text).jsonObject
                            if (!response.isSuccessful) {
                                val detail = result["error"]?.jsonObject
                                throw CloudError(response.code, detail?.text("code") ?: "http${response.code}", detail?.text("message"))
                            }
                            result
                        }
                        continuation.resumeWith(Result.success(parsed))
                    } catch (error: Exception) { continuation.resumeWith(Result.failure(error)) }
                }
            })
        }
        return value
    }

    companion object { private const val MAX_RESPONSE = 2L * 1024 * 1024 }
}
