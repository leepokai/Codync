package com.codync.android

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import com.codync.android.design.*

/** System permission gates the computer preference without changing its saved value. */
@Composable internal fun ComputerNotificationsRow(enabled: Boolean, onChange: (Boolean) -> Unit) {
    val context = LocalContext.current
    var permissionResult by remember { mutableStateOf<Boolean?>(null) }
    val allowedOnResume = rememberOnResume {
        permissionResult = null
        notificationsAllowed(context)
    }
    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        notificationsAnswered(context, granted, PromptSource.Settings)
        permissionResult = notificationsAllowed(context)
        if (granted) PushDelivery.schedule(context)
    }
    ComputerNotificationsControl(enabled, permissionResult ?: allowedOnResume, onChange) {
        if (!PushPreferences.asked(context) && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            request.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            openNotificationSettings(context)
        }
    }
}

@Composable internal fun ComputerNotificationsControl(
    enabled: Boolean,
    allowed: Boolean,
    onChange: (Boolean) -> Unit,
    openPermission: () -> Unit,
) {
    if (allowed) {
        CodyncSwitch(enabled, onChange, Modifier.fillMaxWidth()) {
            RowLabel("Push notifications", "bell", modifier = Modifier.weight(1f))
        }
    } else {
        Column(Modifier.fillMaxWidth().plainButton(action = openPermission)) {
            RowLabel("Push notifications", "bell", chevron = true)
            Text(tr("Turn on notifications in Settings"), style = Type.footnote, color = Palette.secondary)
        }
    }
}
