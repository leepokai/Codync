package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
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
    val claimedAt: Long? = null, val access: String? = null,
    /** Set when Codync Cloud runs this computer (older clouds omit it). */
    val cloud: CloudPlacement? = null) {
    /** This device was approved on it through the account. */
    val isGranted: Boolean get() = access == "granted"
}

/** Who runs a Codync Cloud computer, where, and whether it's awake (`cloud` on an account computer; iOS `CloudHosting`). */
@Serializable
data class CloudPlacement(val provider: String, val providerName: String, val region: String, val state: String) {
    /** Asleep, or going to sleep: it wakes when a device connects or a message arrives. */
    val asleep: Boolean get() = state == "stopped" || state == "stopping"
    val failed: Boolean get() = state == "failed"
}

/** One of the account's cloud computers (`/v1/cloud-computers`, iOS `CloudMachine`): [computerId] stays null until it has
 * claimed itself into the account. */
@Serializable
data class CloudMachine(val cloudComputerId: String,
    /** The name the user gave it (also its account computer's). */
    val name: String,
    val provider: String, val providerName: String, val region: String, val state: String,
    val computerId: String? = null, val backupAt: Long? = null, val createdAt: Long? = null,
    /** Whether its provider keeps snapshots (earlier states, a backup) that Delete snapshots erases; older clouds don't
     * say, and they all did. */
    val snapshots: Boolean = true) {
    val failed: Boolean get() = state == "failed"
    /** Created, and not yet in the account (nor failed): it's booting and installing the host. */
    val preparing: Boolean get() = computerId == null && !failed
}

/**
 * `/v1/cloud-computers`: the account's cloud computers (oldest first), whether it may have them (the feature isn't public
 * yet) and how many, and every provider they can run on with the account's key for it (iOS `CloudStatus`).
 */
@Serializable
data class CloudOffer(@SerialName("cloudComputers") val machines: List<CloudMachine> = emptyList(), val available: Boolean = false,
    val limit: Int = 3, val providers: List<CloudProvider> = emptyList()) {
    /** The provider New cloud computer creates on: [picked] while its key works, else the first whose key does. One picked
     * before its key was added (or fixed) is chosen once it is (iOS `CloudMachineStore.provider(picked:)`). */
    fun chosenProvider(picked: String?): String? {
        val usable = providers.filter { it.usable }
        return (usable.firstOrNull { it.provider == picked } ?: usable.firstOrNull())?.provider
    }

    /** A key can go unless a cloud computer runs on that provider (the cloud needs it to put them to sleep). */
    fun canRemoveKey(provider: String) = machines.none { it.provider == provider }

    /** What a new computer is called when the user names it nothing: [base] while no computer has that name, else
     * [numbered] (`… {n}`) with the smallest free n from 2. The app passes both translated. */
    fun suggestedName(base: String = "Cloud computer", numbered: String = "Cloud computer {n}"): String {
        val taken = machines.mapTo(mutableSetOf()) { it.name }
        if (base !in taken) return base
        return generateSequence(2) { it + 1 }.map { numbered.replace("{n}", "$it") }.first { it !in taken }
    }

    fun withCredential(provider: String, credential: CloudCredential?) =
        copy(providers = providers.map { if (it.provider == provider) it.copy(credential = credential) else it })
}

/**
 * A provider the cloud can run the computer on, as it lists them: where it runs, how to make its key (worded by the
 * cloud, so a new provider needs no app release) and the account's key for it.
 */
@Serializable
data class CloudProvider(val provider: String, val name: String, val region: String,
    /** What its keys start with, for the field's placeholder (`boat_…`). */
    val keyPrompt: String,
    val setup: CloudKeySetup, val credential: CloudCredential? = null,
    /** Its own free plan, if it has one (older clouds don't say). */
    val offer: CloudProviderOffer? = null) {
    /** The account gave it a key that isn't turned down for its own sake: a computer can be made there (or tried again,
     * when the provider said no for the plan or the account). */
    val usable: Boolean get() = credential != null && !credential.blocking
}

/** A provider's own free plan, in the cloud's words (the app translates [label] and [detail]), and its page. */
@Serializable
data class CloudProviderOffer(val label: String, val detail: String, val url: String)

/** A key the account gave a provider, as the cloud shows it: never the key itself. */
@Serializable
data class CloudCredential(
    /** Its kind and last four, `boat_…9abc`. */
    val hint: String,
    /** What the owner has to fix while the provider turns the key down, in a sentence the app translates. */
    val problem: String? = null,
    /** The key itself was turned down (revoked, expired): nothing can be made with it until it's replaced. */
    val blocking: Boolean = false)

/** How the user makes a provider's key: commands to run, or the provider's page. */
@Serializable
data class CloudKeySetup(val page: String, val commands: String)

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
    val json: JsonObject get() = buildJsonObject { put("payload", payload); put("sig", sig) }
}

/** What a computer that joins this phone's account by pairing signs, so the account takes it in. */
data class PairingClaim(val claimId: String, val nonce: String) {
    /** As the host reads it (`pair`'s `claim`, `joinAccount`). */
    val json: JsonObject get() = buildJsonObject { put("claimId", claimId); put("nonce", nonce) }
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
class CloudClient private constructor(base: String, private val identity: DeviceIdentity, private val appVersion: String,
    private val token: suspend () -> String, private val http: OkHttpClient,
    private val pollInterval: Long) {
    private val baseUrl = base.trimEnd('/').toHttpUrl()
    private val json = Json { ignoreUnknownKeys = true }
    private val app = appHeader(appVersion)

    constructor(base: String, identity: DeviceIdentity, appVersion: String, token: suspend () -> String) : this(base, identity, appVersion, token,
        OkHttpClient.Builder().followRedirects(false).followSslRedirects(false).callTimeout(20, TimeUnit.SECONDS).build(), 2_000) {
        require(isCloudUrl(base)) { "Invalid cloud address" }
    }

    internal constructor(base: String, identity: DeviceIdentity, token: suspend () -> String,
        http: OkHttpClient, pollInterval: Long, testOnly: Unit, appVersion: String = "2.24.0") : this(base, identity, appVersion, token, http, pollInterval)

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

    /**
     * Signing out: this device keeps its key and access, but the account's computers stop its alerts until it registers
     * again, signing back in (iOS `signOutThisDevice`).
     */
    suspend fun signOutThisDevice() {
        request("POST", listOf("v1", "devices", "sign-out"), buildJsonObject { }, signed = true)
    }

    /** Starting over: this device leaves the account, and with it every grant and pending request (iOS `revokeThisDevice`). */
    suspend fun revokeThisDevice() {
        request("DELETE", listOf("v1", "devices", deviceId() ?: return))
    }

    /**
     * This client on the session token it would send now, for a call that goes out after signing out: a session token
     * stays valid until it expires (about a minute). Null when there's no token to take (iOS `pinningToken`).
     */
    suspend fun pinningToken(): CloudClient? {
        val now = try { token() } catch (error: CancellationException) { throw error } catch (_: Exception) { return null }
        return CloudClient(baseUrl.toString(), identity, appVersion, { now }, http, pollInterval)
    }

    /** This device's id in the account; null when it was never registered. */
    private suspend fun deviceId(): String? = request("GET", listOf("v1", "devices")).getValue("devices").jsonArray
        .map { it.jsonObject }.firstOrNull { it.text("deviceKey") == identity.publicKey }?.text("deviceId")

    suspend fun accessStatus(id: String): AccessStatus {
        val result = request("GET", listOf("v1", "access-requests", id))
        return json.decodeFromJsonElement(result)
    }
    suspend fun cancelAccess(id: String) { request("DELETE", listOf("v1", "access-requests", id)) }

    /** The account's Codync Cloud computers, and whether this account may create them. */
    suspend fun cloudComputers(): CloudOffer = json.decodeFromJsonElement(request("GET", listOf("v1", "cloud-computers")))
    /**
     * Gives Codync Cloud the key to the user's own account with [provider]. The cloud checks it there and keeps it
     * sealed; a refusal (`422 keyRefused`) says what to fix.
     */
    suspend fun saveCloudKey(provider: String, key: String): CloudCredential = json.decodeFromJsonElement(
        request("PUT", listOf("v1", "cloud-credentials", provider), buildJsonObject { put("key", key.trim()) }).getValue("credential"))
    /** Forgets [provider]'s key (`409 conflict` while a cloud computer runs on it). */
    suspend fun deleteCloudKey(provider: String) { request("DELETE", listOf("v1", "cloud-credentials", provider)) }
    /** Creates another cloud computer called [name] on [provider] (`403 forbidden` when the cloud has Codync Cloud off,
     * `409` at the limit or without that provider's key, `keyRefused` with what to fix). */
    suspend fun createCloudComputer(provider: String, name: String): CloudMachine = machine(
        request("POST", listOf("v1", "cloud-computers"), buildJsonObject { put("provider", provider); put("name", name) }))
    /** Its new [name], here and as its computer (its host takes it at its next look at the cloud). */
    suspend fun renameCloudComputer(id: String, name: String): CloudMachine =
        machine(request("PATCH", listOf("v1", "cloud-computers", id), buildJsonObject { put("name", name) }))
    /** Erases the machine, its snapshots and backup, and takes its computer out of the account. */
    suspend fun deleteCloudComputer(id: String) { request("DELETE", listOf("v1", "cloud-computers", id)) }
    /** Erases its earlier saved states and backup, keeping the computer (`409 conflict` while it sleeps). */
    suspend fun deleteCloudSnapshots(id: String) { request("DELETE", listOf("v1", "cloud-computers", id, "snapshots")) }

    private fun machine(response: JsonObject): CloudMachine = json.decodeFromJsonElement(response.getValue("cloudComputer"))

    private suspend fun request(method: String, path: List<String>, body: JsonObject? = null, signed: Boolean = false): JsonObject {
        val url = baseUrl.newBuilder().apply { path.forEach(::addPathSegment) }.build()
        val bytes = body?.toString()?.toByteArray() ?: byteArrayOf()
        val jwt = token()
        val builder = Request.Builder().url(url).header("Authorization", "Bearer $jwt").header("Codync-App", app)
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
