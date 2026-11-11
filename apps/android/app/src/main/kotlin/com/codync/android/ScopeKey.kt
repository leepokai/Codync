package com.codync.android

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey

/**
 * The account's AES-256 Keystore key, shared by the device identity and the mirror sealed beside
 * it. It never leaves the secure hardware, and reset deletes it with the account's folder.
 */
internal object ScopeKey {
    fun alias(scope: String) = "codync.identity.$scope"

    @Synchronized
    fun get(scope: String): SecretKey {
        val alias = alias(scope)
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (keyStore.getKey(alias, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256).build())
        return generator.generateKey()
    }
}
