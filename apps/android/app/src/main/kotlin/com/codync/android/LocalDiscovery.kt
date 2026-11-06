package com.codync.android

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import com.codync.android.core.DiscoveredComputer
import java.util.concurrent.ConcurrentHashMap

/**
 * Finds computers in no account on the local network (docs/plans/optional-sign-in.md; iOS `LocalDiscovery`). They have
 * no relay to tell a phone their new address, so they advertise it. Nothing found here is trusted: the phone dials the
 * address and the handshake checks the computer's pinned key.
 */
internal object LocalDiscovery {
    /** Also in the host's `remote/discovery.rs`. */
    private const val SERVICE = "_codync._tcp."

    private val services = ConcurrentHashMap<String, NsdServiceInfo>()
    private val found = ConcurrentHashMap<String, DiscoveredComputer>()
    private val waiting = ArrayDeque<NsdServiceInfo>()
    private var manager: NsdManager? = null
    private var resolving = false

    /**
     * The addresses [id] advertised when last asked. The first call starts browsing. Each call asks again for the next try:
     * a computer with a new address keeps its name, and browsing doesn't report that.
     */
    fun urls(context: Context, id: String): List<String> {
        synchronized(this) { if (manager == null) start(context) }
        services.values.forEach(::resolve)
        return found.values.firstOrNull { it.id == id }?.urls.orEmpty()
    }

    private fun start(context: Context) {
        val nsd = context.applicationContext.getSystemService(NsdManager::class.java) ?: return
        manager = nsd
        nsd.discoverServices(SERVICE, NsdManager.PROTOCOL_DNS_SD, object : NsdManager.DiscoveryListener {
            override fun onServiceFound(info: NsdServiceInfo) {
                services[info.serviceName] = info
                resolve(info)
            }
            override fun onServiceLost(info: NsdServiceInfo) {
                services.remove(info.serviceName)
                found.remove(info.serviceName)
            }
            override fun onDiscoveryStarted(type: String) = Unit
            override fun onDiscoveryStopped(type: String) = Unit
            override fun onStartDiscoveryFailed(type: String, code: Int) = stopped(code)
            override fun onStopDiscoveryFailed(type: String, code: Int) = stopped(code)
        })
    }

    private fun stopped(code: Int) {
        android.util.Log.i("Codync", "Local discovery stopped: $code")
        synchronized(this) { manager = null }
    }

    /** One resolve at a time: the platform refuses a second while one runs. */
    private fun resolve(info: NsdServiceInfo) {
        val next = synchronized(this) {
            if (resolving) {
                if (waiting.none { it.serviceName == info.serviceName }) waiting.addLast(info)
                return
            }
            resolving = true
            info
        }
        @Suppress("DEPRECATION") // Its replacement needs API 34; this one still works there.
        manager?.resolveService(next, object : NsdManager.ResolveListener {
            override fun onServiceResolved(resolved: NsdServiceInfo) {
                val attributes = resolved.attributes
                DiscoveredComputer.of(attributes["id"]?.decodeToString(), attributes["urls"]?.decodeToString())
                    ?.let { found[resolved.serviceName] = it }
                resolveNext()
            }
            override fun onResolveFailed(failed: NsdServiceInfo, code: Int) = resolveNext()
        })
    }

    private fun resolveNext() {
        val next = synchronized(this) {
            resolving = false
            waiting.removeFirstOrNull()
        }
        next?.let(::resolve)
    }
}
