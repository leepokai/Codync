package com.codync.android

import android.Manifest
import android.content.pm.PackageManager
import android.hardware.camera2.CameraManager
import android.os.Handler
import android.os.Looper
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.design.CodyncTheme
import java.util.concurrent.ConcurrentHashMap
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

class CameraScannerTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private val hasCamera get() = context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)

    @Test fun theViewfinderAsksForTheCameraInPlaceBeforeOpeningIt() {
        assumeTrue(hasCamera)
        assumeTrue(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED)
        compose.setContent { CodyncTheme { QrViewfinder({ fail("No code without a camera") }, Modifier.fillMaxSize()) } }
        compose.onNodeWithText("Allow the camera to scan the code on your computer.").assertIsDisplayed()
        compose.onNodeWithText("Allow camera").assertIsDisplayed().assertHasClickAction()
        compose.onNodeWithText("Camera settings").assertDoesNotExist()
    }

    @Test fun aDeviceWithoutACameraPointsToThePasteField() {
        assumeTrue(!hasCamera)
        compose.setContent { CodyncTheme { QrViewfinder({}, Modifier.fillMaxSize()) } }
        compose.onNodeWithText("Camera not available. Paste the link below.").assertIsDisplayed()
        compose.onNodeWithText("Allow camera").assertDoesNotExist()
    }

    /** Explicit emulator gate. The ordinary suite does not silently grant physical camera access. */
    @Test fun aRealCameraClosesInBackgroundReopensAndReleasesWhenTheViewfinderLeaves() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("inspectCamera") == "true")
        assertTrue("Native camera fixture belongs on an emulator", android.os.Build.HARDWARE in listOf("ranchu", "goldfish"))
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        assumeTrue(hasCamera)
        instrumentation.uiAutomation.grantRuntimePermission(context.packageName, Manifest.permission.CAMERA)
        val manager = context.getSystemService(CameraManager::class.java)
        val unavailable = ConcurrentHashMap.newKeySet<String>()
        val callback = object : CameraManager.AvailabilityCallback() {
            override fun onCameraUnavailable(cameraId: String) { unavailable.add(cameraId) }
            override fun onCameraAvailable(cameraId: String) { unavailable.remove(cameraId) }
        }
        manager.registerAvailabilityCallback(callback, Handler(Looper.getMainLooper()))
        val visible = mutableStateOf(true)
        val errors = mutableListOf<String>()
        try {
            compose.setContent { CodyncTheme {
                if (visible.value) QrViewfinder({ fail("The empty emulator camera must not find a pairing code") }, Modifier.fillMaxSize()) { it?.let(errors::add) }
                else Text("Scanner closed")
            } }
            compose.waitUntil(15_000) { unavailable.isNotEmpty() }
            assertEquals(emptyList<String>(), errors)
            compose.activityRule.scenario.moveToState(Lifecycle.State.CREATED)
            compose.waitUntil(15_000) { unavailable.isEmpty() }
            compose.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
            compose.waitUntil(15_000) { unavailable.isNotEmpty() }
            compose.runOnUiThread { visible.value = false }
            compose.onNodeWithText("Scanner closed").assertIsDisplayed()
            compose.waitUntil(15_000) { unavailable.isEmpty() }
        } finally {
            compose.runOnUiThread { visible.value = false }
            manager.unregisterAvailabilityCallback(callback)
        }
    }
}
