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

object HostConnector {
    /**
     * Pairs with [certificate] as the account proof; a computer in no account first asks its user (up to two minutes),
     * then signs [claim] to join the phone's account.
     */
    suspend fun pair(pairing: Pairing, identity: DeviceIdentity, deviceName: String, certificate: DeviceCertificate,
        claim: PairingClaim? = null): Paired {
        require(deviceName.isNotBlank() && deviceName.codePointCount(0, deviceName.length) <= 100)
        val channel = connect(pairing.computer, identity, pairing)
        try {
            val response = try {
                channel.call("pair", buildJsonObject {
                    put("code", pairing.code); put("name", deviceName); put("platform", "android")
                    put("certificate", certificate.json)
                    claim?.let { put("claim", buildJsonObject { put("claimId", it.claimId); put("nonce", it.nonce) }) }
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

    suspend fun connect(computer: Computer, identity: DeviceIdentity): EncryptedChannel = connect(computer, identity, null)

    private suspend fun connect(computer: Computer, identity: DeviceIdentity, pairing: Pairing?): EncryptedChannel {
        computer.validate()
        if (computer.route == ConnectionRoute.CloudflareFirst && computer.cloud != null) {
            try { return relay(computer, identity, pairing) }
            catch (error: CancellationException) { throw error }
            catch (error: SecurityException) { throw error }
            catch (_: IOException) { /* Try the direct candidates below. */ }
        }
        var directError: Throwable? = null
        var ownedWinner: EncryptedChannel? = null
        val direct = try { coroutineScope {
            val attempts = Channel<Result<EncryptedChannel>>(Channel.UNLIMITED,
                onUndeliveredElement = { result -> result.getOrNull()?.close() })
            val jobs = computer.urls.map { base -> launch {
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
                    repeat(computer.urls.size) {
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
        if (computer.cloud != null && computer.route == ConnectionRoute.Automatic) return relay(computer, identity, pairing)
        throw directError ?: IOException("Can't reach the computer. Check its connection and try again.")
    }

    private suspend fun relay(computer: Computer, identity: DeviceIdentity, pairing: Pairing?): EncryptedChannel {
        val base = requireNotNull(computer.cloud).toHttpUrl()
        val url = base.newBuilder().addPathSegments("v1/relay/device/${computer.id}").query("v=1")
            .apply { if (pairing != null) addQueryParameter("pair", pairing.offerId) }.build()
        var socket: ChannelSocket? = null
        for (attempt in 0..3) {
            val header = RequestSigner.header(identity, "GET", url)
            val request = Request.Builder().url(url).header("Codync-Sig", header).build()
            try { socket = OkHttpChannelSocket.dial(request); break }
            catch (error: SocketRefused) {
                if (error.status == 426) throw HostError(426, "Update Codync to connect to this computer.")
                if (error.status != 403) throw error
                if (attempt == 3) throw HostError(403, "This device isn't allowed on the computer. Pair it again.")
                // The new device can reach the relay before the host's post-pairing ACL does.
                delay((attempt + 1) * 500L)
            }
        }
        return EncryptedChannel(requireNotNull(socket), HostRoute.Relay, computer, identity, pairing != null).start()
    }
}
