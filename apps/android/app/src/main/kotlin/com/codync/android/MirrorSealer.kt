package com.codync.android

import android.util.Base64
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec

/**
 * A mirror's payloads at rest: AES-GCM under the account's own Keystore key (`ScopeKey`, the one
 * the device identity uses). A sealed value is base64 `iv || ciphertext`, and the column and the
 * account are its AAD, so a value copied to another column or account won't open.
 */
internal class MirrorSealer(private val scope: String) {
    // The Keystore lookup is the slow part; the key outlives no reset (its folder goes with it).
    private val key by lazy { ScopeKey.get(scope) }

    fun seal(column: String, text: String): String {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key)
        cipher.updateAAD(aad(column))
        return Base64.encodeToString(cipher.iv + cipher.doFinal(text.toByteArray()), Base64.NO_WRAP)
    }

    fun open(column: String, sealed: String): String {
        val bytes = Base64.decode(sealed, Base64.NO_WRAP)
        require(bytes.size >= IV_BYTES + TAG_BYTES) { "A sealed mirror value is damaged." }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, bytes.copyOf(IV_BYTES)))
        cipher.updateAAD(aad(column))
        return cipher.doFinal(bytes.copyOfRange(IV_BYTES, bytes.size)).toString(Charsets.UTF_8)
    }

    private fun aad(column: String) = "$scope:$column".toByteArray()

    private companion object {
        const val IV_BYTES = 12
        const val TAG_BYTES = 16
    }
}
