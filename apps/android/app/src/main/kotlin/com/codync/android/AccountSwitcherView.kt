package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle

/** Who you're signed in as, and the other accounts on this phone (iOS `AccountSwitcherView`). */
@Composable internal fun AccountSwitcherView(state: AppState, store: AppStore, pair: () -> Unit) {
    val account = state.account
    val current = account.accounts.firstOrNull { it.userId == account.userId }
    val busy = "session" in state.busy
    var confirmSignOut by remember { mutableStateOf(false) }
    var addingAccount by remember { mutableStateOf(false) }
    LaunchedEffect(account.userId) { addingAccount = false }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Account") { if (busy) ThinkingOrb(ThinkingOrbState.Connecting, 20.dp, modifier = Modifier.semantics { contentDescription = "Signing in" }) }
        CardForm(Modifier.weight(1f)) {
            Row(Modifier.fillMaxWidth().clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent).plainButton(action = pair).padding(16.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Symbol("qrcode.viewfinder", size = 24.dp, tint = Palette.text)
                Text("Scan a computer's code", Modifier.weight(1f), style = Type.body.medium, color = Palette.text)
                Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
            }
            // The big picture: your face and address when signed in, both login options when not.
            Column(Modifier.fillMaxWidth().padding(vertical = 8.dp), horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(if (current != null) 10.dp else 16.dp)) {
                if (current != null) {
                    AccountAvatar(current.avatar, current.email, 76.dp)
                    Text(current.email, style = Type.subheadline.medium, color = Palette.text, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
                } else {
                    Box(Modifier.size(76.dp).clip(Capsule()).background(Palette.bubbleAgent).clearAndSetSemantics {}, contentAlignment = Alignment.Center) {
                        Symbol("person.fill", size = 34.dp, tint = Palette.tertiary)
                    }
                    if (account.configured) SignInButtons(state, store)
                    else IconLabel("QR pairing only in this build", "qrcode", Palette.secondary)
                }
            }
            if (current != null && (account.multiple || account.accounts.size > 1)) {
                Switcher(state, store, busy) { addingAccount = !addingAccount }
                AnimatedVisibility(addingAccount, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
                    exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) { SignInButtons(state, store) }
            }
            if (current != null) CardSection {
                row {
                    RowLabel("Sign out", "rectangle.portrait.and.arrow.right", Palette.danger,
                        modifier = Modifier.plainButton(enabled = !busy) { confirmSignOut = true })
                }
            }
            account.error?.let { IconLabel(it, "exclamationmark.triangle.fill", Palette.danger) }
        }
    }
    CodyncDialog("Sign out of ${current?.email ?: "this account"}?", confirmSignOut, { confirmSignOut = false },
        "This phone forgets the account's computers. Your bots stay on them.") {
        listOf(DialogAction("Sign out", destructive = true, action = store::signOut))
    }
}

/** Every account on this phone as a face; tap one to switch, + to add another. */
@Composable private fun Switcher(state: AppState, store: AppStore, busy: Boolean, add: () -> Unit) {
    Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterHorizontally), verticalAlignment = Alignment.CenterVertically) {
        for (item in state.account.accounts) {
            val current = item.userId == state.account.userId
            Box(Modifier.clip(Capsule()).border(2.dp, if (current) Palette.accent else Color.Transparent, Capsule())
                .plainButton(enabled = !busy && !current) { store.switchAccount(item) }
                .semantics { contentDescription = item.email; selected = current }.padding(3.dp)) {
                AccountAvatar(item.avatar, item.email, 44.dp)
            }
        }
        if (state.account.multiple) Box(Modifier.size(50.dp).clip(Capsule()).background(Palette.bubbleAgent).plainButton(enabled = !busy, action = add)
            .semantics { contentDescription = "Add account" }, contentAlignment = Alignment.Center) {
            Symbol("plus", size = 20.dp, tint = Palette.secondary)
        }
    }
}

/** The account's photo; its first letter on a tinted circle until (or unless) the photo loads. */
@Composable internal fun AccountAvatar(url: String?, email: String?, size: Dp) {
    Box(Modifier.size(size).clip(Capsule()).background(Palette.bubbleUser).clearAndSetSemantics {}, contentAlignment = Alignment.Center) {
        val letter = email?.firstOrNull()
        if (letter != null) Text(letter.uppercase(), style = Type.system(size.value * 0.42f, FontWeight.SemiBold), color = Palette.text)
        else Symbol("person.fill", size = size * 0.5f, tint = Palette.tertiary)
        if (url != null) AsyncImage(url, null, imageLoader = LocalNetworkImages.current, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
    }
}

/** The account face of the roster's top bar. */
@Composable internal fun AccountAvatar(state: AppState, size: Dp) {
    val current = state.account.accounts.firstOrNull { it.userId == state.account.userId }
    AccountAvatar(current?.avatar, current?.email, size)
}
