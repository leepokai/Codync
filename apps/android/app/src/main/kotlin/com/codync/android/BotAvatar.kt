package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.design.*

/** A bot's character, or a group's members. */
@Composable internal fun BotAvatar(bot: Bot, bots: List<Bot>, size: Dp = 44.dp) {
    if (bot.kind == "group") GroupAvatar(bot.memberBots(bots).map { AvatarFace(it.avatarShape, it.avatarColor, it.status) }, size)
    else CharacterAvatar(bot.avatarShape, bot.avatarColor, size, bot.status)
}

/** The roster's avatar, with its status dot (iOS `AvatarWithStatus`): amber needs you, the accent dot is unread. */
@Composable internal fun AvatarWithStatus(bot: Bot, bots: List<Bot>, size: Dp = 44.dp) {
    Box(Modifier.size(size)) {
        BotAvatar(bot, bots, size)
        if (bot.status == "needsInput") Box(Modifier.size(size * .36f).align(if (bot.lead) Alignment.TopEnd else Alignment.BottomEnd).ring()
            .background(Palette.warning, CircleShape), contentAlignment = Alignment.Center) {
            Symbol("exclamationmark", size = size * .24f, tint = Color.White)
        } else if (bot.unread > 0) Box(Modifier.size(size * .28f).align(if (bot.lead) Alignment.TopEnd else Alignment.BottomEnd).ring().background(Palette.accentFill, CircleShape))
        if (bot.lead) Box(Modifier.align(Alignment.BottomEnd)) { LeadAgentBadge(size * .34f) }
    }
}

/** A 2 dp ring in the background color around the dot, centered on its edge as iOS strokes it. */
@Composable private fun Modifier.ring(): Modifier {
    val color = Palette.background
    return drawWithContent {
        drawContent()
        drawCircle(color, size.minDimension / 2, style = Stroke(2.dp.toPx()))
    }
}
