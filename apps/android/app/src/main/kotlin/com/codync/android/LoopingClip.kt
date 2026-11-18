package com.codync.android

import android.content.res.AssetManager
import android.graphics.SurfaceTexture
import android.media.MediaPlayer
import android.util.Log
import android.view.Surface
import android.view.TextureView
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import com.codync.android.design.reduceMotion

/**
 * A short silent clip from the app's assets that plays in a loop, filling its frame (What's new; iOS `LoopingClip`).
 * With animations turned off in the system settings it rests on its first frame.
 */
@Composable internal fun LoopingClip(asset: String, modifier: Modifier = Modifier) {
    val assets = LocalContext.current.assets
    val still = reduceMotion()
    val clip = remember(asset) { ClipPlayer(assets, asset, still) }
    // The view keeps its surface while the app is in the background: nothing decodes there.
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) { clip.inFront = false }
    LifecycleEventEffect(Lifecycle.Event.ON_START) { clip.inFront = true }
    AndroidView({ context -> TextureView(context).apply { surfaceTextureListener = clip } }, modifier)
}

/** The clip's player, alive as long as its view's surface. Main thread only. */
private class ClipPlayer(private val assets: AssetManager, private val asset: String, private val still: Boolean) : TextureView.SurfaceTextureListener {
    private var player: MediaPlayer? = null
    private var surface: Surface? = null
    private var prepared = false

    /** Plays only while the app is in front. */
    var inFront = true
        set(value) {
            field = value
            val player = player?.takeIf { prepared && !still } ?: return
            if (value) player.start() else player.pause()
        }

    override fun onSurfaceTextureAvailable(texture: SurfaceTexture, width: Int, height: Int) {
        val made = MediaPlayer()
        val output = Surface(texture)
        try {
            assets.openFd(asset).use { made.setDataSource(it.fileDescriptor, it.startOffset, it.length) }
            made.setSurface(output)
            made.isLooping = true
            made.setVolume(0f, 0f)
            made.setOnPreparedListener {
                prepared = true
                if (still || !inFront) it.seekTo(0) else it.start()
            }
            made.prepareAsync()
            player = made
            surface = output
        } catch (e: Exception) {
            Log.e("LoopingClip", "$asset failed to play", e)
            made.release()
            output.release()
        }
    }

    override fun onSurfaceTextureDestroyed(texture: SurfaceTexture): Boolean {
        player?.release()
        surface?.release()
        player = null
        surface = null
        prepared = false
        return true
    }

    override fun onSurfaceTextureSizeChanged(texture: SurfaceTexture, width: Int, height: Int) {}
    override fun onSurfaceTextureUpdated(texture: SurfaceTexture) {}
}
