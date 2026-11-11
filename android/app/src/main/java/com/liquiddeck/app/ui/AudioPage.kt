package com.liquiddeck.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.VolumeOff
import androidx.compose.material.icons.rounded.BluetoothAudio
import androidx.compose.material.icons.rounded.CloudOff
import androidx.compose.material.icons.rounded.Headphones
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Speaker
import androidx.compose.material.icons.rounded.SpeakerGroup
import androidx.compose.material.icons.rounded.Tv
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.liquiddeck.app.data.AudioDevice
import com.liquiddeck.app.data.AudioSnapshot
import com.liquiddeck.app.ui.theme.Blue
import com.liquiddeck.app.ui.theme.Cyan
import com.liquiddeck.app.ui.theme.Green
import com.liquiddeck.app.ui.theme.Red
import com.liquiddeck.app.ui.theme.TextMuted
import com.liquiddeck.app.ui.theme.TextPrimary
import java.util.Locale

/**
 * Audio-output page: every render endpoint the PC reports, one tap to make it
 * the default. Reads `/api/audio` through the shared connection and switches
 * with the same `output` operation the web mixer and the deck keys use.
 */
@Composable
fun AudioPage(
    state: AudioUi,
    hapticsEnabled: Boolean,
    onRefresh: () -> Unit,
    onSelect: (String) -> Unit,
) {
    val snapshot = state.snapshot
    val haptics = LocalHapticFeedback.current

    BoxWithConstraints(Modifier.fillMaxSize()) {
        // Two columns once there is room for a readable card; a single column
        // on portrait phones keeps the names from wrapping into three lines.
        val columns = if (maxWidth >= 620.dp) 2 else 1

        Column(Modifier.fillMaxSize()) {
            Header(snapshot = snapshot, loading = state.loading, onRefresh = onRefresh)

            Box(
                Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .padding(horizontal = 18.dp),
            ) {
                when {
                    snapshot == null && state.error != null -> Message(
                        icon = Icons.Rounded.CloudOff,
                        title = "Ses cihazları okunamadı",
                        body = state.error,
                        color = Red,
                        onRefresh = onRefresh,
                    )

                    snapshot == null -> Box(
                        Modifier.fillMaxSize(),
                        contentAlignment = Alignment.Center,
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(16.dp),
                                strokeWidth = 2.dp,
                                color = Cyan,
                            )
                            Spacer(Modifier.width(10.dp))
                            Text("Cihazlar okunuyor…", fontSize = 11.sp, color = TextMuted)
                        }
                    }

                    !snapshot.supported -> Message(
                        icon = Icons.AutoMirrored.Rounded.VolumeOff,
                        title = "Ses değiştirme kullanılamıyor",
                        body = snapshot.message
                            ?: "PC'de ses arka ucuna erişilemedi.",
                        color = TextMuted,
                        onRefresh = onRefresh,
                    )

                    snapshot.devices.isEmpty() -> Message(
                        icon = Icons.Rounded.SpeakerGroup,
                        title = "Etkin çıkış yok",
                        body = "PC'ye bir kulaklık, hoparlör veya HDMI ekranı bağla.",
                        color = TextMuted,
                        onRefresh = onRefresh,
                    )

                    else -> LazyVerticalGrid(
                        columns = GridCells.Fixed(columns),
                        contentPadding = PaddingValues(bottom = 12.dp),
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        items(snapshot.devices, key = { it.id }) { device ->
                            DeviceCard(
                                device = device,
                                supportsVolume = snapshot.supportsVolume,
                                switching = state.switching == device.id,
                                onClick = {
                                    if (!device.isDefault && state.switching == null) {
                                        if (hapticsEnabled) {
                                            haptics.performHapticFeedback(
                                                HapticFeedbackType.LongPress,
                                            )
                                        }
                                        onSelect(device.id)
                                    }
                                },
                            )
                        }
                    }
                }
            }

            if (snapshot != null && snapshot.supported && snapshot.devices.isNotEmpty()) {
                Text(
                    text = hintFor(snapshot),
                    fontSize = 9.5.sp,
                    color = TextMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(horizontal = 22.dp, vertical = 6.dp),
                )
            }
        }
    }
}

@Composable
private fun Header(
    snapshot: AudioSnapshot?,
    loading: Boolean,
    onRefresh: () -> Unit,
) {
    val current = snapshot?.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = 22.dp, end = 10.dp, top = 2.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                text = "SES ÇIKIŞI",
                style = MaterialTheme.typography.labelSmall,
                color = Cyan,
            )
            Spacer(Modifier.height(2.dp))
            Text(
                text = current?.description
                    ?: if (snapshot == null) "Cihazlar okunuyor…" else "Bir çıkış seç",
                style = MaterialTheme.typography.titleMedium,
                color = TextPrimary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }

        if (snapshot != null && snapshot.devices.isNotEmpty()) {
            CountPill("${snapshot.devices.size} çıkış · ${snapshot.backend}")
            Spacer(Modifier.width(4.dp))
        }

        IconButton(onClick = onRefresh) {
            if (loading && snapshot == null) {
                CircularProgressIndicator(
                    modifier = Modifier.size(15.dp),
                    strokeWidth = 2.dp,
                    color = Cyan,
                )
            } else {
                Icon(
                    Icons.Rounded.Refresh,
                    contentDescription = "Yenile",
                    tint = TextMuted,
                    modifier = Modifier.size(19.dp),
                )
            }
        }
    }
}

@Composable
private fun CountPill(text: String) {
    Text(
        text = text.uppercase(Locale.getDefault()),
        fontSize = 9.sp,
        letterSpacing = 0.5.sp,
        color = TextMuted,
        maxLines = 1,
        modifier = Modifier
            .clip(RoundedCornerShape(50))
            .background(Color.White.copy(alpha = 0.06f))
            .padding(horizontal = 9.dp, vertical = 4.dp),
    )
}

@Composable
private fun DeviceCard(
    device: AudioDevice,
    supportsVolume: Boolean,
    switching: Boolean,
    onClick: () -> Unit,
) {
    val accent = if (device.isDefault) Green else null
    GlassCard(
        modifier = Modifier.fillMaxWidth(),
        accent = accent,
        onClick = onClick,
        contentPadding = PaddingValues(horizontal = 13.dp, vertical = 11.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                modifier = Modifier
                    .size(40.dp)
                    .clip(RoundedCornerShape(14.dp))
                    .background((if (device.isDefault) Green else Blue).copy(alpha = 0.12f)),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    imageVector = iconFor(device),
                    contentDescription = null,
                    tint = if (device.isDefault) Green else Cyan,
                    modifier = Modifier.size(20.dp),
                )
            }

            Spacer(Modifier.width(12.dp))

            Column(Modifier.weight(1f)) {
                Text(
                    text = device.description.ifBlank { "Ses çıkışı" },
                    fontSize = 14.sp,
                    fontWeight = FontWeight.Medium,
                    color = TextPrimary,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(3.dp))
                Text(
                    text = subtitle(device, supportsVolume),
                    fontSize = 9.5.sp,
                    letterSpacing = 0.5.sp,
                    color = TextMuted,
                    maxLines = 1,
                )
                if (supportsVolume && !device.muted) {
                    Spacer(Modifier.height(7.dp))
                    ProgressLine(
                        progress = device.volume / 100f,
                        accent = if (device.isDefault) Green.copy(alpha = 0.75f) else Cyan.copy(alpha = 0.55f),
                        height = 3,
                    )
                }
            }

            Spacer(Modifier.width(10.dp))

            when {
                switching -> CircularProgressIndicator(
                    modifier = Modifier.size(16.dp),
                    strokeWidth = 2.dp,
                    color = Cyan,
                )

                device.isDefault -> Text(
                    text = "VARSAYILAN",
                    fontSize = 8.5.sp,
                    letterSpacing = 0.6.sp,
                    color = Green,
                    maxLines = 1,
                )

                else -> Text(
                    text = "Seç",
                    fontSize = 11.sp,
                    color = Cyan,
                    maxLines = 1,
                )
            }
        }
    }
}

@Composable
private fun Message(
    icon: ImageVector,
    title: String,
    body: String,
    color: Color,
    onRefresh: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(bottom = 10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Icon(icon, contentDescription = null, tint = color, modifier = Modifier.size(26.dp))
        Spacer(Modifier.height(8.dp))
        Text(
            text = title,
            fontSize = 14.sp,
            fontWeight = FontWeight.Medium,
            color = TextPrimary,
        )
        Spacer(Modifier.height(4.dp))
        Text(
            text = body,
            fontSize = 11.sp,
            color = TextMuted,
        )
        Spacer(Modifier.height(10.dp))
        Text(
            text = "Yenile",
            fontSize = 11.sp,
            color = Cyan,
            modifier = Modifier
                .clip(RoundedCornerShape(50))
                .background(Cyan.copy(alpha = 0.12f))
                .clickable(onClick = onRefresh)
                .padding(horizontal = 14.dp, vertical = 6.dp),
        )
    }
}

private fun iconFor(device: AudioDevice): ImageVector = when (device.kind) {
    "Bluetooth" -> Icons.Rounded.BluetoothAudio
    "Kulaklık" -> Icons.Rounded.Headphones
    "HDMI" -> Icons.Rounded.Tv
    "USB" -> Icons.Rounded.Speaker
    else -> Icons.Rounded.SpeakerGroup
}

private fun subtitle(device: AudioDevice, supportsVolume: Boolean): String {
    val kind = device.kind.uppercase(Locale.getDefault())
    return when {
        device.muted -> "$kind · SESSİZ"
        supportsVolume && device.volume == 100 -> "$kind · TAM"
        supportsVolume -> "$kind · %${device.volume}"
        else -> kind
    }
}

private fun hintFor(snapshot: AudioSnapshot): String = when (snapshot.backend) {
    "pactl" -> "Seçim varsayılan çıkışı değiştirir; çalan akışlar yeni cihaza taşınır."
    "WASAPI" -> "Seçim Windows'un varsayılan çıkışını değiştirir; uygulamalar anında geçer."
    else -> "Seçim PC'deki varsayılan çıkışı değiştirir."
}
