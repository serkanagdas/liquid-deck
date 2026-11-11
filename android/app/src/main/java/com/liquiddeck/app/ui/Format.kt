package com.liquiddeck.app.ui

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material.icons.rounded.Calculate
import androidx.compose.material.icons.rounded.ChatBubble
import androidx.compose.material.icons.rounded.Code
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.material.icons.rounded.FiberManualRecord
import androidx.compose.material.icons.rounded.Folder
import androidx.compose.material.icons.rounded.Forum
import androidx.compose.material.icons.rounded.GridView
import androidx.compose.material.icons.rounded.Headphones
import androidx.compose.material.icons.rounded.Keyboard
import androidx.compose.material.icons.rounded.Language
import androidx.compose.material.icons.rounded.Mic
import androidx.compose.material.icons.rounded.MicOff
import androidx.compose.material.icons.rounded.Monitor
import androidx.compose.material.icons.rounded.MusicNote
import androidx.compose.material.icons.rounded.Pause
import androidx.compose.material.icons.rounded.PhotoCamera
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Podcasts
import androidx.compose.material.icons.rounded.PowerSettingsNew
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.SkipNext
import androidx.compose.material.icons.rounded.SkipPrevious
import androidx.compose.material.icons.rounded.SportsEsports
import androidx.compose.material.icons.rounded.Terminal
import androidx.compose.material.icons.rounded.Videocam
import androidx.compose.material.icons.rounded.VolumeDown
import androidx.compose.material.icons.rounded.VolumeOff
import androidx.compose.material.icons.rounded.VolumeUp
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import com.liquiddeck.app.ui.theme.Blue
import com.liquiddeck.app.ui.theme.Cyan
import com.liquiddeck.app.ui.theme.Green
import com.liquiddeck.app.ui.theme.Orange
import com.liquiddeck.app.ui.theme.Pink
import com.liquiddeck.app.ui.theme.Purple
import com.liquiddeck.app.ui.theme.Red
import com.liquiddeck.app.ui.theme.Slate
import com.liquiddeck.app.ui.theme.Yellow
import java.util.Locale
import kotlin.math.roundToInt

fun deckColor(raw: String): Color {
    return when (raw.lowercase(Locale.US)) {
        "cyan" -> Cyan
        "blue" -> Blue
        "orange" -> Orange
        "violet", "purple" -> Purple
        "pink" -> Pink
        "green" -> Green
        "red" -> Red
        "yellow" -> Yellow
        "slate", "gray", "grey" -> Slate
        else -> {
            val hex = raw.removePrefix("#")
            if (hex.length == 6) {
                hex.toLongOrNull(16)?.let { Color(0xFF000000L or it) } ?: Cyan
            } else {
                Cyan
            }
        }
    }
}

fun iconFor(name: String): ImageVector? = when (name.lowercase(Locale.US)) {
    "mic" -> Icons.Rounded.Mic
    "mic-off" -> Icons.Rounded.MicOff
    "headphones" -> Icons.Rounded.Headphones
    "headphones-off" -> Icons.Rounded.VolumeOff
    "gamepad" -> Icons.Rounded.SportsEsports
    "record" -> Icons.Rounded.FiberManualRecord
    "volume-up" -> Icons.Rounded.VolumeUp
    "volume-down" -> Icons.Rounded.VolumeDown
    "volume" -> Icons.Rounded.VolumeUp
    "camera", "screenshot" -> Icons.Rounded.PhotoCamera
    "chat", "discord" -> Icons.Rounded.Forum
    "terminal" -> Icons.Rounded.Terminal
    "music" -> Icons.Rounded.MusicNote
    "grid" -> Icons.Rounded.GridView
    "code", "editor" -> Icons.Rounded.Code
    "bolt" -> Icons.Rounded.Bolt
    "play" -> Icons.Rounded.PlayArrow
    "pause" -> Icons.Rounded.Pause
    "next" -> Icons.Rounded.SkipNext
    "prev", "previous" -> Icons.Rounded.SkipPrevious
    "power" -> Icons.Rounded.PowerSettingsNew
    "settings" -> Icons.Rounded.Settings
    "browser" -> Icons.Rounded.Language
    "files" -> Icons.Rounded.Folder
    "calculator" -> Icons.Rounded.Calculate
    "edit" -> Icons.Rounded.Edit
    "monitor" -> Icons.Rounded.Monitor
    "obs", "stream" -> Icons.Rounded.Videocam
    "podcast" -> Icons.Rounded.Podcasts
    "keyboard" -> Icons.Rounded.Keyboard
    "chat-bubble" -> Icons.Rounded.ChatBubble
    else -> null
}

/** Icon names the key editor offers; each one resolves through [iconFor]. */
val deckIconNames: List<String> = listOf(
    "bolt",
    "mic",
    "mic-off",
    "headphones",
    "headphones-off",
    "gamepad",
    "record",
    "volume-up",
    "volume-down",
    "music",
    "play",
    "pause",
    "next",
    "prev",
    "terminal",
    "code",
    "browser",
    "files",
    "calculator",
    "chat",
    "monitor",
    "obs",
    "power",
    "settings",
    "grid",
    "camera",
    "keyboard",
)

fun formatBytes(bytes: Long): String {
    if (bytes <= 0) return "0 B"
    val units = arrayOf("B", "KB", "MB", "GB", "TB")
    var value = bytes.toDouble()
    var index = 0
    while (value >= 1024 && index < units.lastIndex) {
        value /= 1024
        index++
    }
    return if (index == 0 || value >= 100) {
        "${value.roundToInt()} ${units[index]}"
    } else {
        String.format(Locale.US, "%.1f %s", value, units[index])
    }
}

fun formatRate(bytesPerSecond: Double): String = "${formatBytes(bytesPerSecond.toLong())}/s"

fun formatUptime(seconds: Long): String {
    val days = seconds / 86_400
    val hours = (seconds % 86_400) / 3_600
    val minutes = (seconds % 3_600) / 60
    return when {
        days > 0 -> "${days}g ${hours}s"
        hours > 0 -> "${hours}s ${minutes}dk"
        else -> "${minutes}dk"
    }
}

fun formatDuration(milliseconds: Long): String {
    val total = (milliseconds / 1000).coerceAtLeast(0)
    val hours = total / 3600
    val minutes = (total % 3600) / 60
    val seconds = total % 60
    return if (hours > 0) {
        String.format(Locale.US, "%d:%02d:%02d", hours, minutes, seconds)
    } else {
        String.format(Locale.US, "%d:%02d", minutes, seconds)
    }
}
