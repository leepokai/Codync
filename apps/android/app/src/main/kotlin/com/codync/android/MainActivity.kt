package com.codync.android

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.net.Uri
import android.os.Bundle
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels

import androidx.core.content.ContextCompat
import androidx.core.content.IntentCompat
import androidx.fragment.app.FragmentActivity

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import coil3.ImageLoader
import coil3.disk.DiskCache
import com.codync.android.core.ShareIntake
import com.codync.android.design.CodyncTheme
import com.codync.android.design.AppearancePreferences
import com.codync.android.design.OverlayRoot
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okio.Path.Companion.toOkioPath

class MainActivity : FragmentActivity() {
    private val store: AppStore by viewModels()

    /** A screen that just lit up is not the person deciding: see [Approvals]. */
    private val screenOn = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            Approvals.screenLit()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ContextCompat.registerReceiver(
            this,
            screenOn,
            IntentFilter(Intent.ACTION_SCREEN_ON),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        applyWindowOrientation(resources.configuration)
        AppearancePreferences.get(applicationContext)
        ThinkingDetails.initialize(applicationContext)
        NativeAccounts.initialize(this)
        Analytics.start(applicationContext)
        Analytics.userId = { store.state.value.account.userId }
        NotificationDisplay.channels(this)
        if (savedInstanceState == null) lifecycleScope.launch { CodyncWidgets.publishPreviews(applicationContext) }
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
            LaunchedEffect(state.bots, state.computer?.id, state.contextId, state.rosterLoaded, state.loading) {
                val computer = state.computer ?: return@LaunchedEffect
                if (state.rosterLoaded && !state.loading) withContext(Dispatchers.IO) {
                    LauncherShortcuts.sync(applicationContext, state.contextId, computer.id, state.bots)
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
    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        if (store.state.value.screenRequest == null) applyWindowOrientation(newConfig)
    }
    private fun applyWindowOrientation(config: Configuration) {
        val target = if (config.smallestScreenWidthDp >= 600) {
            ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
        } else {
            ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
        }
        if (requestedOrientation != target) requestedOrientation = target
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
    override fun onResume() {
        super.onResume()
        // Approvals are tapped on this screen: an activity that isn't on screen can't ask for a thumb.
        Approvals.attach(this)
    }
    override fun onPause() {
        Approvals.detach(this)
        super.onPause()
    }
    override fun onDestroy() {
        unregisterReceiver(screenOn)
        super.onDestroy()
    }
    override fun onStart() { super.onStart(); store.setForeground(true); Analytics.becameActive(this) }
    override fun onStop() {
        store.setForeground(false)
        if (!isChangingConfigurations) SensitiveAccess.revoke()
        // A rotation isn't a trip to the background.
        if (!isChangingConfigurations) Analytics.enteredBackground()
        super.onStop()
    }
    /** A system share: copy what it carries now, while its grant lasts, then let the user pick the bot. Returns whether it was one. */
    private fun handleShare(intent: Intent): Boolean {
        if (intent.action != Intent.ACTION_SEND && intent.action != Intent.ACTION_SEND_MULTIPLE) return false
        // Consumed: a rotation or process restoration must not share it again.
        setIntent(Intent(this, MainActivity::class.java))
        if (!ShareIntake.accepts(intent.type)) return true
        val uris = if (intent.action == Intent.ACTION_SEND) listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
            else IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
        val text = ShareIntake.draftText(intent.getStringExtra(Intent.EXTRA_SUBJECT), intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString())
        if (text.isNotEmpty() || uris.isNotEmpty()) store.receiveShare(text, uris)
        return true
    }
    private fun handle(intent: Intent?) {
        if (intent != null && handleShare(intent)) return
        val uri = intent?.data ?: return
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host == "pair") {
            intent.data = null
            store.pair(uri.toString())
        }
        // Retain the scoped, non-secret destination for Activity/process restoration.
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host in setOf("bot", "usage")) store.openLink(uri)
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host == "oauth") { intent.data = null; store.finishConnectorSignIn(uri) }
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host == "screen") { intent.data = null; store.openScreenLink() }
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host == "computers") { intent.data = null; store.openPage(LinkPage.Computers) }
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host == "plugins") { intent.data = null; store.openPage(LinkPage.Extensions) }
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
