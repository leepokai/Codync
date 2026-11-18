package com.codync.android

import android.app.Instrumentation
import com.codync.android.core.RealtimePeer
import kotlinx.coroutines.runBlocking

/** The minified WebRTC an OpenAI Realtime call (and the remote screen) opens: its native code builds Java objects by name. */
object ReleaseWebRtcSmoke {
    @JvmStatic fun run(instrumentation: Instrumentation) = runBlocking {
        val peer = WebRtcPeer(instrumentation.targetContext)
        try {
            val offer = peer.offer(object : RealtimePeer.Events {
                override fun opened() {}
                override fun received(text: String) {}
                override fun ice(state: RealtimePeer.Ice) {}
            })
            check("m=audio" in offer && "m=application" in offer) { "The minified WebRTC offer lacks audio or the events channel" }
        } finally { peer.close() }
    }
}
