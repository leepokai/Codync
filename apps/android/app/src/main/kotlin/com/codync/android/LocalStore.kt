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
class LocalStore(private val context: Context, val contextId: String = "local") {
    private val scopeId = MessageDigest.getInstance("SHA-256").digest(contextId.toByteArray())
        .joinToString("") { "%02x".format(it) }
    val referenceId = referenceId(contextId)
    internal val directory = File(context.noBackupFilesDir, scopeId).apply { mkdirs() }
    private val alias = "codync.identity.$scopeId"
    private val json = Json { ignoreUnknownKeys = true }

    @Synchronized
    fun identity(): DeviceIdentity {
        val seeds = openSeeds() ?: ByteArray(64).also(SecureRandom()::nextBytes).also(::sealSeeds)
        try {
            require(seeds.size == 64)
            return DeviceIdentity(seeds.copyOfRange(0, 32), seeds.copyOfRange(32, 64))
        } finally { seeds.fill(0) }
    }

    /** The device's key seeds, opened with this scope's Keystore key; null before the first [identity]. */
    private fun openSeeds(): ByteArray? {
        val file = AtomicFile(File(directory, "identity"))
        if (!file.baseFile.exists()) return null
        val sealed = file.readFully()
        require(sealed.size == 12 + 64 + 16) { "Device identity is damaged. Start over to pair again." }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, wrappingKey(), GCMParameterSpec(128, sealed.copyOf(12)))
        cipher.updateAAD(scopeId.toByteArray())
        return cipher.doFinal(sealed.copyOfRange(12, sealed.size))
    }

    private fun sealSeeds(seeds: ByteArray) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, wrappingKey())
        cipher.updateAAD(scopeId.toByteArray())
        write(AtomicFile(File(directory, "identity")), cipher.iv + cipher.doFinal(seeds))
    }

    /** What [adopt] did: how many computers came over, and whether the device keys came with them. */
    internal data class Adoption(val computers: Int, val keys: MovedKeys)

    /**
     * Sign-in is required: [from] (the signed-out context) moves into this account context, then is erased (iOS
     * `SharedStore.Context.adopt(from:)`). New here (no identity, no computers), this context takes everything, its device
     * keys too (sealed again under this scope), so computers keep seeing the same device. Otherwise, or when that move
     * fails, only the computer list merges (this context's entry wins), and those computers ask to be paired again. Null
     * when there was nothing to move; a failure to merge leaves [from] as it was. Run before this context's stores and identity load.
     */
    @Synchronized
    internal fun adopt(from: LocalStore): Adoption? {
        val moving = from.computers()
        if (moving.isEmpty() && !from.hasIdentity()) return null
        val own = computers()
        val keys = if (!hasIdentity() && own.isEmpty() && takeAll(from)) MovedKeys.Moved else {
            val known = own.map { it.id }.toSet()
            saveComputers(own + moving.filter { it.id !in known })
            MovedKeys.Rescan
        }
        from.erase()
        return Adoption(moving.size, keys)
    }

    /** [from]'s device keys and files, for [adopt]; false when they couldn't move (the computers merge instead). */
    private fun takeAll(from: LocalStore): Boolean = try {
        from.openSeeds()?.let { seeds -> try { sealSeeds(seeds) } finally { seeds.fill(0) } }
        // Not the sealed identity or AtomicFile's leftovers of it: only [sealSeeds] writes this scope's.
        for (file in from.directory.listFiles().orEmpty().filter { !it.name.startsWith("identity") }) {
            file.copyRecursively(File(directory, file.name), overwrite = true)
        }
        // Drafts and queued messages name their files by full path: point them at the copies here.
        for (computer in from.computers()) {
            DraftBook(directory, computer.id).relocate(from.directory)
            MirrorDatabase.relocate(context, directory, computer.id, from.directory)
        }
        true
    } catch (error: Exception) {
        android.util.Log.w("Codync", "Couldn't move the signed-out device key", error)
        false
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
