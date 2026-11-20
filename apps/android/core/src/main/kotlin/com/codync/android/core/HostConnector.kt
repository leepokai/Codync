package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.Request

/** A computer pairing let in, and the claim it signed when it joined this phone's account (null when it didn't). */
data class Paired(val computer: Computer, val claim: JsonObject?)

private const val UNREACHABLE_DIRECTLY = "Can't reach your computer. Check you're on the same network or that Tailscale is on."

/** Nothing answered at the address, as opposed to a host that turned this phone away. */
private val Throwable?.isNoAnswer get() = this == null || this is java.net.SocketException ||
    this is java.net.UnknownHostException || this is java.io.InterruptedIOException

object HostConnector {
    /**
     * Pairs with [certificate] as the account proof; a computer in no account first asks its user (up to 90 seconds),
     * then signs [claim] to join the phone's account. `appVersion` goes to the Codync cloud with a relay connection ([appHeader]).
     */
    suspend fun pair(pairing: Pairing, identity: DeviceIdentity, deviceName: String, certificate: DeviceCertificate?,
        claim: PairingClaim? = null, appVersion: String? = null): Paired {
        require(deviceName.isNotBlank() && deviceName.codePointCount(0, deviceName.length) <= 100)
        val channel = connect(pairing.computer, identity, pairing, appVersion)
        try {
            val response = try {
                channel.call("pair", buildJsonObject {
                    put("code", pairing.code); put("name", deviceName); put("platform", "android")
                    certificate?.let { put("certificate", it.json) }
                    claim?.let { put("claim", it.json) }
                }, timeoutMillis = 150_000)
            } catch (error: HostError) {
                // Hosts from before Android support turn the platform down: say what to do, not the wire error.
                if (error.status == 400 && "platform" in error.message) throw HostError(400, "Update Codync on this computer to pair an Android phone.")
                throw error
            }
            val result = response.jsonObject
            require(result.text("computerId") == pairing.computer.id) { "This computer's identity changed. Pair it again." }
            return Paired(pairing.computer.copy(name = result.text("name")?.takeIf(String::isNotBlank) ?: pairing.computer.name),
                result["claim"] as? JsonObject)
        } finally {
            channel.close()
        }
    }

    /** [discovered]: addresses found on the local network, for a computer with no relay to learn them from. */
    suspend fun connect(computer: Computer, identity: DeviceIdentity, appVersion: String? = null,
        discovered: List<String> = emptyList()): EncryptedChannel =
        connect(computer, identity, null, appVersion, discovered)

    private suspend fun connect(computer: Computer, identity: DeviceIdentity, pairing: Pairing?, appVersion: String?,
        discovered: List<String> = emptyList()): EncryptedChannel {
        computer.validate()
        if (computer.route == ConnectionRoute.CloudflareFirst && computer.cloud != null) {
            try { return relay(computer, identity, pairing, appVersion) }
            catch (error: CancellationException) { throw error }
            catch (error: SecurityException) { throw error }
            catch (_: IOException) { /* Try the direct candidates below. */ }
        }
        val candidates = (computer.urls + discovered).distinct()
        var directError: Throwable? = null
        var ownedWinner: EncryptedChannel? = null
        val direct = try { coroutineScope {
            val attempts = Channel<Result<EncryptedChannel>>(Channel.UNLIMITED,
                onUndeliveredElement = { result -> result.getOrNull()?.close() })
            val jobs = candidates.map { base -> launch {
                var channel: EncryptedChannel? = null
                try {
                    val url = base.toHttpUrl().newBuilder().encodedPath("/channel").query("v=1").build()
                    val socket = OkHttpChannelSocket.dial(Request.Builder().url(url).build())
                    channel = EncryptedChannel(socket, HostRoute.Direct, computer, identity, pairing != null)
                    val started = channel.start()
                    attempts.send(Result.success(started))
                    channel = null // Ownership passed to the result queue.
                } catch (error: CancellationException) {
                    throw error
                } catch (error: Throwable) {
                    attempts.send(Result.failure(error))
                } finally { channel?.close() }
            } }
            try {
                val budget = if (computer.route == ConnectionRoute.DirectOnly || computer.cloud == null) 15_000L else 1_500L
                withTimeoutOrNull(budget) {
                    var winner: EncryptedChannel? = null
                    repeat(candidates.size) {
                        if (winner == null) {
                            val outcome = attempts.receive()
                            winner = outcome.getOrNull()
                            ownedWinner = winner
                            directError = outcome.exceptionOrNull() ?: directError
                        }
                    }
                    winner
                }
            } finally {
                jobs.forEach { it.cancel() }
                attempts.cancel()
            }
        } } catch (error: Throwable) { ownedWinner?.close(); throw error }
        if (direct != null) return direct
        if (directError is SecurityException) throw requireNotNull(directError)
        if (computer.cloud != null && computer.route == ConnectionRoute.Automatic) return relay(computer, identity, pairing, appVersion)
        // With no relay, a computer that's off and a phone that's away look the same. An answer from the host says more.
        if (directError.isNoAnswer && (computer.cloud == null || computer.route == ConnectionRoute.DirectOnly)) {
            throw IOException(UNREACHABLE_DIRECTLY)
        }
        throw directError ?: IOException("Can't reach the computer. Check its connection and try again.")
    }

    private suspend fun relay(computer: Computer, identity: DeviceIdentity, pairing: Pairing?, appVersion: String?): EncryptedChannel {
        var socket: ChannelSocket? = null
        for (attempt in 0..3) {
            try { socket = OkHttpChannelSocket.dial(relayRequest(computer, identity, pairing, appVersion)); break }
            catch (error: SocketRefused) {
                if (error.status == 426) throw HostError(426, error.appFloor ?: "Update Codync to connect to this computer.")
                if (error.status != 403) throw error
                if (attempt == 3) throw HostError(403, "This device isn't allowed on the computer. Pair it again.")
                // The new device can reach the relay before the host's post-pairing ACL does.
                delay((attempt + 1) * 500L)
            }
        }
        return EncryptedChannel(requireNotNull(socket), HostRoute.Relay, computer, identity, pairing != null).start()
    }

    /** The relay upgrade, signed afresh for each attempt. */
    internal fun relayRequest(computer: Computer, identity: DeviceIdentity, pairing: Pairing?, appVersion: String?): Request {
        val url = requireNotNull(computer.cloud).toHttpUrl().newBuilder().addPathSegments("v1/relay/device/${computer.id}").query("v=1")
            .apply { if (pairing != null) addQueryParameter("pair", pairing.offerId) }.build()
        return Request.Builder().url(url).header("Codync-Sig", RequestSigner.header(identity, "GET", url))
            .apply { if (appVersion != null) header("Codync-App", appHeader(appVersion)) }.build()
    }
}
