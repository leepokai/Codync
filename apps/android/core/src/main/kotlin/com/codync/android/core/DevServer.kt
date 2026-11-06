package com.codync.android.core

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/**
 * A program a bot keeps running on the computer: the host runs it ([managed]), or the bot left it running through its
 * own shell. What each state shows and offers is decided here, the same on the iPhone (`DevServer.swift`).
 */
@Serializable
data class DevServer(
    val id: String,
    val name: String = "",
    val command: String = "",
    val folder: String? = null,
    val status: DevServerStatus = DevServerStatus.Stopped,
    val ports: List<DevServerPort> = emptyList(),
    val startedAt: Long? = null,
    val exitCode: Int? = null,
    val managed: Boolean = true,
) {
    /** Where a phone can reach it while it runs, best first: the first port that has any. */
    val remoteAddresses: List<String> get() =
        if (status == DevServerStatus.Running) ports.firstOrNull { it.remote.isNotEmpty() }?.remote.orEmpty() else emptyList()

    /** The best of them; null when it listens only on its computer. */
    val remoteAddress: String? get() = remoteAddresses.firstOrNull()

    /** Its row's value: its ports, or a phrase to look up. */
    val rowValue: String get() = when (status) {
        DevServerStatus.Starting -> "Starting"
        DevServerStatus.Stopped -> "Stopped"
        DevServerStatus.Failed -> "Failed to start"
        DevServerStatus.Running -> if (ports.isEmpty()) "Running" else ports.joinToString(" ") { ":${it.port}" }
    }

    /** Open what can be reached, or read the log of what has no port (a watch build, or one that ended). */
    val tap: DevServerTap? get() = remoteAddress?.let(DevServerTap::Open)
        ?: DevServerTap.Logs.takeIf { managed && !(status == DevServerStatus.Running && ports.isNotEmpty()) }

    /**
     * Only what applies. A server the bot started through its shell can only be opened or stopped. A tap opens the first
     * address, which this phone may not reach (a computer's Tailscale address, without Tailscale here), so when there are
     * more each one is a row.
     */
    val menuRows: List<DevServerMenuRow> get() = buildList {
        val live = status == DevServerStatus.Running || status == DevServerStatus.Starting
        if (status == DevServerStatus.Running && ports.isNotEmpty() && remoteAddress == null) add(DevServerMenuRow.OnlyOnComputer)
        if (remoteAddresses.size > 1) remoteAddresses.forEach { add(DevServerMenuRow.OpenAddress(it)) }
        else if (remoteAddress != null) add(DevServerMenuRow.Open)
        if (remoteAddress != null) add(DevServerMenuRow.CopyAddress)
        if (managed) {
            add(DevServerMenuRow.Logs)
            if (status != DevServerStatus.Starting) add(if (live) DevServerMenuRow.Restart else DevServerMenuRow.Start)
        } else add(DevServerMenuRow.StartedByShell)
        if (live) add(DevServerMenuRow.Stop) else if (managed) add(DevServerMenuRow.Remove)
    }
}

@Serializable
data class DevServerPort(
    val port: Int,
    /** Works on the computer running it. */
    val local: String = "",
    /** Work from other devices; empty when it listens only on that computer. */
    val remote: List<String> = emptyList(),
)

@Serializable(with = DevServerStatusSerializer::class)
enum class DevServerStatus(val wire: String) { Starting("starting"), Running("running"), Stopped("stopped"), Failed("failed") }

/** A status a newer host adds reads as stopped, rather than losing the bot. */
internal object DevServerStatusSerializer : KSerializer<DevServerStatus> {
    override val descriptor = PrimitiveSerialDescriptor("DevServerStatus", PrimitiveKind.STRING)
    override fun serialize(encoder: Encoder, value: DevServerStatus) = encoder.encodeString(value.wire)
    override fun deserialize(decoder: Decoder): DevServerStatus {
        val wire = decoder.decodeString()
        return DevServerStatus.entries.firstOrNull { it.wire == wire } ?: DevServerStatus.Stopped
    }
}

/** What the host can be asked to do with a server. Clients name it and never send a command. */
enum class DevServerAction(val method: String) { Stop("devServerStop"), Restart("devServerRestart"), Remove("devServerRemove") }

/** What tapping its row does. */
sealed interface DevServerTap {
    data class Open(val address: String) : DevServerTap
    data object Logs : DevServerTap
}

/** A line of its menu. */
sealed interface DevServerMenuRow {
    data object OnlyOnComputer : DevServerMenuRow
    data object Open : DevServerMenuRow
    data class OpenAddress(val address: String) : DevServerMenuRow {
        /** As the row shows it. */
        val title: String get() = address.removePrefix("http://")
    }
    data object CopyAddress : DevServerMenuRow
    data object Logs : DevServerMenuRow
    data object Restart : DevServerMenuRow
    data object Start : DevServerMenuRow
    data object StartedByShell : DevServerMenuRow
    data object Stop : DevServerMenuRow
    data object Remove : DevServerMenuRow
}

/** What the chat's title shows of a bot's servers: nothing until one starts, or once they've all stopped. */
sealed interface DevServerSummary {
    data object None : DevServerSummary
    data object Starting : DevServerSummary
    data object Failed : DevServerSummary
    data class Running(val text: String) : DevServerSummary
}

fun devServerSummary(servers: List<DevServer>): DevServerSummary {
    val running = servers.filter { it.status == DevServerStatus.Running }
    val first = running.firstOrNull { it.ports.isNotEmpty() } ?: running.firstOrNull()
    return when {
        first != null -> {
            val label = first.ports.firstOrNull()?.let { ":${it.port}" } ?: first.name
            DevServerSummary.Running(if (running.size > 1) "$label +${running.size - 1}" else label)
        }
        servers.any { it.status == DevServerStatus.Starting } -> DevServerSummary.Starting
        servers.any { it.status == DevServerStatus.Failed } -> DevServerSummary.Failed
        else -> DevServerSummary.None
    }
}
