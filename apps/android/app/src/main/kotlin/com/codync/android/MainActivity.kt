package com.codync.android

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.LaunchedEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.codync.android.design.CodyncTheme
import com.codync.android.design.OverlayRoot
import coil3.ImageLoader
import coil3.disk.DiskCache
import java.io.File
import okio.Path.Companion.toOkioPath

class MainActivity : ComponentActivity() {
    private val store: AppStore by viewModels()
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        NativeAccounts.initialize(this)
        Analytics.start(applicationContext)
        Analytics.userId = { store.state.value.account.userId }
        NotificationDisplay.channels(this)
        enableEdgeToEdge()
        setContent {
            val state by store.state.collectAsStateWithLifecycle()
            val notifications = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { store.syncPush() }
            LaunchedEffect(state.computers.isNotEmpty(), state.contextId) {
                if (BuildConfig.PUSH_CONFIGURED && state.computers.isNotEmpty() && android.os.Build.VERSION.SDK_INT >= 33 && !PushPreferences.asked(applicationContext)) {
                    PushPreferences.markAsked(applicationContext)
                    notifications.launch(android.Manifest.permission.POST_NOTIFICATIONS)
                }
            }
            val images = remember(state.contextId, state.generation) { AccountImages.open(applicationContext, state.contextId) }
            DisposableEffect(images) { onDispose { AccountImages.close(images) } }
            CompositionLocalProvider(LocalNetworkImages provides images) {
                key(state.contextId, state.generation) {
                    CodyncTheme { OverlayRoot { CodyncApp(state, store); LaunchLayer(state.loading) } }
                }
            }
        }
        // Rotation keeps its ViewModel/navigation; process restoration gets a fresh owner.
        if (savedInstanceState?.getString("codync:activityOwner") != store.activityOwnerId) handle(intent)
    }
    override fun onSaveInstanceState(outState: Bundle) {
        outState.putString("codync:activityOwner", store.activityOwnerId)
        super.onSaveInstanceState(outState)
    }
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handle(intent)
    }
    override fun onStart() { super.onStart(); store.setForeground(true); Analytics.becameActive() }
    override fun onStop() {
        store.setForeground(false)
        // A rotation isn't a trip to the background.
        if (!isChangingConfigurations) Analytics.enteredBackground()
        super.onStop()
    }
    private fun handle(intent: Intent?) {
        val uri = intent?.data ?: return
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host == "pair") {
            intent.data = null
            store.pair(uri.toString())
        }
        // Retain the scoped, non-secret destination for Activity/process restoration.
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host in setOf("bot", "usage")) store.openLink(uri)
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host == "oauth") { intent.data = null; store.finishConnectorSignIn(uri) }
    }
}

/** One active loader: switching accounts closes old requests before their cache can be erased. */
internal object AccountImages {
    private var current: ImageLoader? = null
    @Synchronized fun open(context: android.content.Context, id: String): ImageLoader {
        retire()
        val images = ImageLoader.Builder(context).diskCache {
            DiskCache.Builder().directory(File(context.cacheDir, "codync-images-$id").toOkioPath()).maxSizeBytes(64L * 1024 * 1024).build()
        }.build()
        current = images
        return images
    }
    @Synchronized fun close(images: ImageLoader) { images.shutdown(); if (current === images) current = null }
    @Synchronized fun retire() { current?.shutdown(); current = null }
}
