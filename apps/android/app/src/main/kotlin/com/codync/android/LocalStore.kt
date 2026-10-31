package com.codync.android

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import com.codync.android.core.Computer
import com.codync.android.core.DeviceIdentity
import java.io.File
import java.security.KeyStore
import java.security.MessageDigest
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import kotlinx.serialization.json.Json

/** Account-scoped, atomic storage excluded from both backup and device transfer. */
class LocalStore(context: Context, val contextId: String = "local") {
    private val scopeId = MessageDigest.getInstance("SHA-256").digest(contextId.toByteArray())
        .joinToString("") { "%02x".format(it) }
    val referenceId = referenceId(contextId)
    internal val directory = File(context.noBackupFilesDir, scopeId).apply { mkdirs() }
    private val alias = "codync.identity.$scopeId"
    private val json = Json { ignoreUnknownKeys = true }

    @Synchronized
    fun identity(): DeviceIdentity {
        val file = AtomicFile(File(directory, "identity"))
        val seeds = if (file.baseFile.exists()) {
            val sealed = file.readFully()
            require(sealed.size == 12 + 64 + 16) { "Device identity is damaged. Start over to pair again." }
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, wrappingKey(), GCMParameterSpec(128, sealed.copyOf(12)))
            cipher.updateAAD(scopeId.toByteArray())
            cipher.doFinal(sealed.copyOfRange(12, sealed.size))
        } else {
            val fresh = ByteArray(64).also(SecureRandom()::nextBytes)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.ENCRYPT_MODE, wrappingKey())
            cipher.updateAAD(scopeId.toByteArray())
            write(file, cipher.iv + cipher.doFinal(fresh))
            fresh
        }
        try {
            require(seeds.size == 64)
            return DeviceIdentity(seeds.copyOfRange(0, 32), seeds.copyOfRange(32, 64))
        } finally { seeds.fill(0) }
    }

    private fun wrappingKey(): SecretKey {
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val existing = keyStore.getKey(alias, null) as? SecretKey
        if (existing != null) return existing
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256).build())
        return generator.generateKey()
    }

    fun computers(): List<Computer> {
        val file = AtomicFile(File(directory, "computers.json"))
        if (!file.baseFile.exists()) return emptyList()
        val computers = json.decodeFromString<List<Computer>>(file.readFully().toString(Charsets.UTF_8))
        computers.forEach(Computer::validate)
        return computers
    }

    fun saveComputers(computers: List<Computer>) {
        computers.forEach(Computer::validate)
        write(AtomicFile(File(directory, "computers.json")), json.encodeToString(computers).toByteArray())
    }

    /** The computer a chat was last opened or sent on (iOS `lastComputerId`). */
    fun lastComputer(): String? = File(directory, "last-computer").takeIf { it.exists() }?.readText()?.takeIf { it.isNotBlank() }
    fun saveLastComputer(id: String) = write(AtomicFile(File(directory, "last-computer")), id.toByteArray())

    /** The computer whose lead "Talk to Codync" calls; none is Automatic. */
    fun shortcutLead(): String? = File(directory, "shortcut-lead").takeIf { it.exists() }?.readText()?.takeIf { it.isNotBlank() }
    fun saveShortcutLead(id: String?) {
        if (id == null) File(directory, "shortcut-lead").delete() else write(AtomicFile(File(directory, "shortcut-lead")), id.toByteArray())
    }

    fun setupComplete(): Boolean = File(directory, "setup-complete").exists()
    fun hasIdentity(): Boolean = File(directory, "identity").exists()
    fun completeSetup() = write(AtomicFile(File(directory, "setup-complete")), byteArrayOf(1))

    fun erase() {
        check(directory.deleteRecursively()) { "Couldn't erase this phone's local data." }
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        keyStore.deleteEntry(alias)
        check(directory.mkdirs()) { "Couldn't create new local storage." }
    }

    companion object {
        private fun write(file: AtomicFile, bytes: ByteArray) {
            val output = file.startWrite()
            try { output.write(bytes); file.finishWrite(output) }
            catch (error: Throwable) { file.failWrite(output); throw error }
        }

        /** Every account's folder (signed out included), by its scope id. */
        private fun scopes(context: Context): List<File> =
            context.noBackupFilesDir.listFiles()?.filter { it.isDirectory && it.name.matches(Regex("[a-f0-9]{64}")) }.orEmpty()

        /** Push switched off on every account's saved computers (`PushPreferences.splitSwitch`). */
        fun pushOffEverywhere(context: Context) {
            val json = Json { ignoreUnknownKeys = true }
            for (file in scopes(context).map { AtomicFile(File(it, "computers.json")) }.filter { it.baseFile.exists() }) {
                val computers = json.decodeFromString<List<Computer>>(file.readFully().toString(Charsets.UTF_8))
                write(file, json.encodeToString(computers.map { it.copy(notifications = false) }).toByteArray())
            }
        }

        fun referenceId(contextId: String): String = if (contextId == "local") "local" else MessageDigest.getInstance("SHA-256")
            .digest(contextId.toByteArray()).joinToString("") { "%02x".format(it) }
        /** Reset includes inactive accounts and their wrapping keys, never host data. */
        fun eraseAll(context: Context) {
            scopes(context).forEach { check(it.deleteRecursively()) { "Couldn't erase local account data." } }
            val keys = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
            keys.aliases().toList().filter { it.startsWith("codync.identity.") }.forEach(keys::deleteEntry)
        }
    }
}
