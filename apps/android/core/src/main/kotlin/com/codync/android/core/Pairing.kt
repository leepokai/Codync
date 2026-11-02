package com.codync.android.core

import java.net.URI
import java.net.URLDecoder

/** Why a pairing link can't be used (iOS `Pairing.Problem`). */
class PairingProblem(val kind: Kind, cause: Throwable? = null) : IllegalArgumentException(kind.message, cause) {
    enum class Kind(val message: String) {
        NotPairingLink("That isn't a Codync pairing code."),
        /** The computer runs a Codync that speaks another protocol version. */
        OutdatedHost("Update Codync on your computer, then show a new pairing code."),
        Invalid("This pairing code is damaged. Show a new one on your computer."),
    }
}

/** A short-lived pairing URL. Only the resulting Computer may be persisted. */
data class Pairing(val computer: Computer, val code: String) {
    internal val offerId: String get() = RelayCrypto.offerId(code.decodeBase64Url(16))

    /** The code the computer shows if it asks whether to let [identity]'s phone in. */
    fun checkCode(identity: DeviceIdentity): String = RelayCrypto.pairCheck(identity.deviceKey, computer.signKey.decodeBase64Url(32))

    companion object {
        private val SHORT_CODE = Regex("([A-Za-z]{4})[- ]?([0-9]{4})")

        /**
         * A short code `codync-host pair` prints (`QRTX-4821`: 4 letters, an optional dash or space, 4 digits) as the cloud
         * takes it, or null for anything else (iOS `Pairing.shortCode`). The cloud resolves it to the pairing link.
         */
        fun shortCode(text: String): String? =
            SHORT_CODE.matchEntire(text.trim())?.destructured?.let { (letters, digits) -> "${letters.uppercase()}-$digits" }

        fun parse(text: String, scheme: String): Pairing {
            if (text.length > 16 * 1024) throw PairingProblem(PairingProblem.Kind.Invalid)
            val url = try { URI(text.trim()) } catch (error: Exception) {
                throw PairingProblem(PairingProblem.Kind.NotPairingLink, error)
            }
            if (url.scheme != scheme || url.host != "pair" || url.rawQuery == null) throw PairingProblem(PairingProblem.Kind.NotPairingLink)
            val params = linkedMapOf<String, String>()
            try {
                url.rawQuery.split('&').forEach { part ->
                    val key = URLDecoder.decode(part.substringBefore('='), "UTF-8")
                    val value = URLDecoder.decode(part.substringAfter('=', ""), "UTF-8")
                    require(params.put(key, value) == null) { "Duplicate pairing field" }
                }
                require(params["v"] == "3") { "Update Codync on your computer, then show a new pairing code." }
                val code = requireNotNull(params["code"])
                code.decodeBase64Url(16)
                val computer = Computer(
                    id = requireNotNull(params["id"]),
                    name = params["name"]?.takeIf(String::isNotBlank) ?: "My computer",
                    signKey = requireNotNull(params["sk"]),
                    boxKey = requireNotNull(params["bk"]),
                    urls = directUrls(params["urls"].orEmpty().split(',')),
                    cloud = params["cloud"]?.takeIf(String::isNotEmpty),
                )
                computer.validate()
                require(computer.urls.isNotEmpty() || computer.cloud != null) { "No valid computer address" }
                return Pairing(computer, code)
            } catch (error: IllegalArgumentException) {
                throw PairingProblem(if (params["v"] != "3") PairingProblem.Kind.OutdatedHost else PairingProblem.Kind.Invalid, error)
            }
        }
    }
}

