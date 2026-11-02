package com.codync.android

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.SideEffect
import androidx.core.view.WindowCompat
import androidx.activity.viewModels
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.LaunchedEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import com.codync.android.design.CodyncTheme
import com.codync.android.design.LocalAppLanguage
import com.codync.android.design.LocalPhrases
import com.codync.android.design.LocalSetAppLanguage
import com.codync.android.design.LocalSystemTag
import com.codync.android.design.LocalSystemChinese
import com.codync.android.design.systemScript
import com.codync.android.design.OverlayRoot
import coil3.ImageLoader
import coil3.disk.DiskCache
import java.io.File
import kotlinx.coroutines.launch
import okio.Path.Companion.toOkioPath

class MainActivity : ComponentActivity() {
    private val accounts: AccountStore by viewModels()
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        AppearancePreferences.initialize(applicationContext)
        ThinkingDetails.initialize(applicationContext)
        NativeAccounts.initialize(this)
        Analytics.start(applicationContext)
        // Clerk's session, not the UI's copy of it: right as sign-in and sign-out happen.
        Analytics.userId = NativeAccounts::activeUserId
        NotificationDisplay.channels(this)
        if (savedInstanceState == null) lifecycleScope.launch { CodyncWidgets.publishPreviews(applicationContext) }
        enableEdgeToEdge()
        setContent {
            val state by accounts.state.collectAsStateWithLifecycle()
            val computers by accounts.computerStates.collectAsStateWithLifecycle()
            val notifications = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
                notificationsAnswered(applicationContext, granted, PromptSource.Automatic)
                accounts.syncPush()
            }
            LaunchedEffect(state.computers.isNotEmpty(), state.contextId) {
                if (BuildConfig.PUSH_CONFIGURED && state.computers.isNotEmpty() && android.os.Build.VERSION.SDK_INT >= 33 && !PushPreferences.asked(applicationContext)) {
                    PushPreferences.markAsked(applicationContext)
                    // Already allowed (in Android settings): no prompt to answer.
                    if (!notificationsAllowed(applicationContext)) notifications.launch(android.Manifest.permission.POST_NOTIFICATIONS)
                }
            }
            val images = remember(state.contextId, state.generation) { AccountImages.open(applicationContext, state.contextId) }
            DisposableEffect(images) { onDispose { AccountImages.close(images) } }
            var appLanguage by remember { mutableStateOf(AppLanguageStore.get(applicationContext)) }
            val systemTag = androidx.compose.ui.platform.LocalConfiguration.current.locales[0].toLanguageTag()
            val phrases = remember(appLanguage, systemTag) { AppLanguageStore.phrases(applicationContext, appLanguage, systemTag) }
            val systemChinese = remember(systemTag) { systemScript(systemTag) != null }
            CompositionLocalProvider(
                LocalNetworkImages provides images,
                LocalAppLanguage provides appLanguage,
                LocalSystemChinese provides systemChinese,
                LocalSystemTag provides systemTag,
                LocalPhrases provides phrases,
                LocalSetAppLanguage provides { next ->
                    appLanguage = next
                    AppLanguageStore.set(applicationContext, next)
                    lifecycleScope.launch {
                        try { CodyncWidgets.update(applicationContext) }
                        catch (cancelled: kotlinx.coroutines.CancellationException) { throw cancelled }
                        catch (error: Exception) { android.util.Log.w("Codync", "Could not refresh widget language", error) }
                    }
                },
            ) {
                key(state.contextId, state.generation) {
                    val appearance by AppearancePreferences.mode.collectAsStateWithLifecycle()
                    val dark = appearance.isDark(isSystemInDarkTheme())
                    val darkBars = dark || computers.values.any { it.screenRequest != null }
                    SideEffect {
                        WindowCompat.getInsetsController(window, window.decorView).apply {
                            isAppearanceLightStatusBars = !darkBars
                            isAppearanceLightNavigationBars = !darkBars
                        }
                    }
                    CodyncTheme(dark = dark) { OverlayRoot { CodyncApp(state, computers, accounts)
                        LaunchLayer(state.loading, state.computers.all { computers[it.id]?.widgetBotsCurrent == true }) } }
                }
            }
        }
        // Rotation keeps its ViewModel/navigation; process restoration gets a fresh owner and the activity's first intent
        // again, as does a return from Recents: its links open again, but a call or a share happened once.
        val replayed = savedInstanceState != null || intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY != 0
        if (savedInstanceState?.getString("codync:activityOwner") != accounts.activityOwnerId &&
            !(replayed && intent.action in setOf(LiveShortcut.ACTION, Intent.ACTION_SEND))) handle(intent)
    }
    override fun onSaveInstanceState(outState: Bundle) {
        outState.putString("codync:activityOwner", accounts.activityOwnerId)
        super.onSaveInstanceState(outState)
    }
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handle(intent)
    }
    override fun onStart() {
        super.onStart(); accounts.setForeground(true); Analytics.becameActive(applicationContext)
        lifecycleScope.launch { AppUpdates.refresh(applicationContext) }
    }
    override fun onStop() {
        accounts.setForeground(false)
        // A rotation isn't a trip to the background.
        if (!isChangingConfigurations) Analytics.enteredBackground()
        super.onStop()
    }
    private fun handle(intent: Intent?) {
        if (intent?.action == LiveShortcut.ACTION) { accounts.talkToLead(); return }
        // Text shared from another app, handed over by `ShareActivity`.
        if (intent?.action == Intent.ACTION_SEND) {
            intent.getStringExtra(Intent.EXTRA_TEXT)?.takeIf { it.isNotBlank() }?.let(accounts::share)
            return
        }
        val uri = intent?.data ?: return
        intent.getStringExtra(NotificationDisplay.ALERT_KIND)?.let { category ->
            intent.removeExtra(NotificationDisplay.ALERT_KIND)
            NotificationKind.of(category)?.let { Analytics.capture(AnalyticsEvent.NotificationOpened(it)) }
        }
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host == "pair") {
            intent.data = null
            accounts.pair(uri.toString(), PairedVia.Link)
        }
        // Retain the scoped, non-secret destination for Activity/process restoration.
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host in setOf("bot", "usage")) accounts.openLink(uri)
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host == "oauth") { intent.data = null; accounts.finishConnectorSignIn(uri) }
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host == "screen") { intent.data = null; accounts.openScreenLink() }
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host == "computers") { intent.data = null; accounts.openPage(LinkPage.Computers) }
        if (uri.scheme == BuildConfig.URL_SCHEME && uri.host == "plugins") { intent.data = null; accounts.openPage(LinkPage.Extensions) }
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
