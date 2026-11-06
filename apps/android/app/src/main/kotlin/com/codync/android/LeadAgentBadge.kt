package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.design.*

@Composable internal fun LeadAgentBadge(size: Dp, interactive: Boolean = false) {
    var showingRole by remember { mutableStateOf(false) }
    val label = tr(if (interactive) "About the lead agent" else "Lead agent")
    val target = if (interactive) Modifier.requiredSize(44.dp).plainButton { showingRole = true } else Modifier.size(size)
    Box(target.semantics { contentDescription = label }, contentAlignment = Alignment.Center) {
        Box(Modifier.size(size).background(Palette.warning, CircleShape), contentAlignment = Alignment.Center) {
            Symbol("star.fill", size = size * .58f, tint = Color.White)
        }
    }
    if (interactive) CodyncSheet(showingRole, { showingRole = false }) {
        Column(Modifier.fillMaxSize().background(Palette.background)) {
            ModalHeader("Lead agent")
            Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp)) {
                Text(tr("Your lead agent is your first point of contact. It follows your goals and unfinished tasks, suggests useful next steps, and coordinates work with your other bots. Its Check in routine runs twice a day by default; you can change or pause it in Routines."), style = Type.body, color = Palette.secondary)
            }
        }
    }
}
