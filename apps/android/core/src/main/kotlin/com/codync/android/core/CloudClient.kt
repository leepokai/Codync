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
    val claimedAt: Long? = null, val access: String? = null) {
    /** This device was approved on it through the account. */
    val isGranted: Boolean get() = access == "granted"
}

data class AccessTicket(val requestId: String, val expiresAt: Long, val code: String, val computerId: String, val signKey: String)

@Serializable
data class AccessStatus(val requestId: String, val status: String, val computerId: String? = null,
    val expiresAt: Long? = null, val hostNonce: String? = null, val grantId: String? = null) {
    // Unrecognized terminal states never authorize the device.
    val state: String get() = status.takeIf { it in setOf("pending", "approved", "denied", "expired", "cancelled") } ?: "expired"
}

/** The cloud's word that this device key is in the account, which `pair` shows the computer (`{payload, sig}`). */
data class DeviceCertificate(val payload: String, val sig: String) {
    /** When it stops being valid (ms), from the signed payload. */
    val expiresAt: Long = Json.parseToJsonElement(payload).jsonObject.getValue("exp").jsonPrimitive.long
    internal val json: JsonObject get() = buildJsonObject { put("payload", payload); put("sig", sig) }
}

/** What a computer that joins this phone's account by pairing signs, so the account takes it in. */
data class PairingClaim(val claimId: String, val nonce: String)

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

    suspend fun deviceCertificate(): DeviceCertificate {
        val certificate = request("POST", listOf("v1", "devices", "certificate"), buildJsonObject { }, signed = true)
            .getValue("certificate").jsonObject
        return DeviceCertificate(requireNotNull(certificate.text("payload")), requireNotNull(certificate.text("sig")))
    }

    /** [cached] while more than a day of it is left, else a new certificate; a key the cloud doesn't know is registered first. */
    suspend fun certificate(cached: DeviceCertificate?, deviceName: String): DeviceCertificate {
        if (cached != null && cached.expiresAt - System.currentTimeMillis() > CERTIFICATE_REUSE) return cached
        return try { deviceCertificate() } catch (error: CloudError) {
            if (error.status != 403) throw error
            registerDevice(deviceName)
            deviceCertificate()
        }
    }

    suspend fun createClaim(): PairingClaim {
        val claim = request("POST", listOf("v1", "claims"), buildJsonObject { })
        return PairingClaim(requireNotNull(claim.text("claimId")), requireNotNull(claim.text("nonce")))
    }

    /** Hands the account [signed], the claim the computer signed when it let this phone in. */
    suspend fun completeClaim(claimId: String, signed: JsonObject) {
        request("POST", listOf("v1", "claims", claimId, "complete"), signed)
    }

    /** The pairing link a short code ([Pairing.shortCode]) stands for. The cloud hands it out once. */
    suspend fun pairingLink(code: String): String =
        requireNotNull(request("GET", listOf("v1", "pairing-codes", code)).text("link")) { "Invalid pairing code response" }

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
        val device = deviceId() ?: return
        val grants = request("GET", listOf("v1", "computers", computerId, "grants")).getValue("grants").jsonArray.map { it.jsonObject }
        for (grant in grants.filter { it.text("deviceId") == device }) {
            request("DELETE", listOf("v1", "computers", computerId, "grants", requireNotNull(grant.text("grantId")) { "Invalid grant" }))
        }
    }

    /** Signing out: this device leaves the account, and with it every grant and pending request (iOS `revokeThisDevice`). */
    suspend fun revokeThisDevice() {
        request("DELETE", listOf("v1", "devices", deviceId() ?: return))
    }

    /**
     * This client on the session token it would send now, for a call that goes out after signing out: a session token
     * stays valid until it expires (about a minute). Null when there's no token to take (iOS `pinningToken`).
     */
    suspend fun pinningToken(): CloudClient? {
        val now = try { token() } catch (error: CancellationException) { throw error } catch (_: Exception) { return null }
        return CloudClient(baseUrl.toString(), identity, { now }, http, pollInterval)
    }

    /** This device's id in the account; null when it was never registered. */
    private suspend fun deviceId(): String? = request("GET", listOf("v1", "devices")).getValue("devices").jsonArray
        .map { it.jsonObject }.firstOrNull { it.text("deviceKey") == identity.publicKey }?.text("deviceId")

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
                            if (!response.isSuccessful) {
                                // A proxy or platform error page isn't JSON; the status still says what happened.
                                val detail = runCatching { json.parseToJsonElement(text).jsonObject["error"]?.jsonObject }.getOrNull()
                                throw CloudError(response.code, detail?.text("code") ?: "http${response.code}", detail?.text("message"))
                            }
                            json.parseToJsonElement(text).jsonObject
                        }
                        continuation.resumeWith(Result.success(parsed))
                    } catch (error: Exception) { continuation.resumeWith(Result.failure(error)) }
                }
            })
        }
        return value
    }

    companion object {
        private const val MAX_RESPONSE = 2L * 1024 * 1024
        private const val CERTIFICATE_REUSE = 24L * 60 * 60 * 1000
    }
}
