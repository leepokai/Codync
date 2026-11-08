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
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.design.*

/** A bot's character, or a group's members; still (idle) unless [animated]. */
@Composable internal fun BotAvatar(bot: Bot, bots: List<Bot>, size: Dp = 44.dp, animated: Boolean = true) {
    fun status(of: Bot) = if (animated) of.status else "idle"
    if (bot.kind == "group" && bot.customIcon == null) GroupAvatar(bot.memberBots(bots).map { AvatarFace(it.avatarShape, it.avatarColor, status(it), it.customIcon?.dataUrl, it.customIcon?.tinted == true) }, size)
    else CharacterAvatar(bot.avatarShape, bot.avatarColor, size, status(bot), customImage = bot.customIcon?.dataUrl, customTinted = bot.customIcon?.tinted == true)
}

/** An agent's logo for an avatar's corner: its ACP registry id, and its name for TalkBack. */
internal class AgentMark(val registry: String, val name: String)

/** The roster's avatar, with its status dot (iOS `AvatarWithStatus`): amber needs you, the accent dot is unread.
 *  [agent] puts the agent's logo in the bottom-start corner. */
@Composable internal fun AvatarWithStatus(bot: Bot, bots: List<Bot>, size: Dp = 44.dp, agent: AgentMark? = null) {
    Box(Modifier.size(size)) {
        BotAvatar(bot, bots, size)
        if (agent != null) Box(Modifier.size(size * .4f).align(Alignment.BottomStart).background(Palette.background, CircleShape)
            .semantics { contentDescription = agent.name }, contentAlignment = Alignment.Center) {
            AgentIcon(agent.registry, size * .28f)
        }
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
