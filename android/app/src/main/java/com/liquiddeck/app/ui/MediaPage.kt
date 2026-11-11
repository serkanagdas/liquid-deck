package com.liquiddeck.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.MusicNote
import androidx.compose.material.icons.rounded.Pause
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.SkipNext
import androidx.compose.material.icons.rounded.SkipPrevious
import androidx.compose.material.icons.rounded.VolumeDown
import androidx.compose.material.icons.rounded.VolumeUp
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import coil.request.ImageRequest
import com.liquiddeck.app.data.MediaSnapshot
import com.liquiddeck.app.ui.theme.Blue
import com.liquiddeck.app.ui.theme.CardRaised
import com.liquiddeck.app.ui.theme.Cyan
import com.liquiddeck.app.ui.theme.Ink
import com.liquiddeck.app.ui.theme.Stroke
import com.liquiddeck.app.ui.theme.TextMuted
import com.liquiddeck.app.ui.theme.TextPrimary
import kotlinx.coroutines.delay

@Composable
fun MediaPage(ui: UiState, vm: DeckViewModel) {
    val media = ui.media
    val artUrl = remember(media?.artVersion, ui.host) {
        media?.artVersion?.takeIf { it > 0 }?.let(vm::artUrl)
    }

    var tick by remember { mutableLongStateOf(0L) }
    LaunchedEffect(media?.playing, media?.title) {
        while (media?.playing == true) {
            tick = System.currentTimeMillis()
            delay(1_000)
        }
    }
    val anchor = remember(media?.title, media?.artist, media?.posMs) {
        System.currentTimeMillis()
    }

    val position = if (media?.playing == true) {
        media.posMs + (tick - anchor).coerceAtLeast(0)
    } else {
        media?.posMs ?: 0
    }
    val duration = media?.durMs ?: 0
    val fraction = if (duration > 0) {
        (position.toFloat() / duration).coerceIn(0f, 1f)
    } else {
        0f
    }
    val hasMedia = media != null && media.title.isNotBlank()

    BoxWithConstraints(Modifier.fillMaxSize()) {
        // Landscape: artwork left, copy and controls right, like the web panel.
        if (maxWidth > maxHeight && maxWidth >= 560.dp) {
            val compact = maxHeight < 320.dp
            val artSize = (maxHeight - 28.dp).coerceAtMost(320.dp).coerceAtLeast(120.dp)

            Row(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 30.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(28.dp),
            ) {
                Artwork(
                    artUrl = artUrl,
                    corner = 26.dp,
                    iconSize = 56.dp,
                    modifier = Modifier.size(artSize),
                )

                Column(
                    modifier = Modifier
                        .weight(1f)
                        .verticalScroll(rememberScrollState()),
                    horizontalAlignment = Alignment.Start,
                ) {
                    TrackDetails(
                        media = media,
                        align = TextAlign.Start,
                        showAlbum = !compact,
                    )

                    Spacer(Modifier.height(if (compact) 12.dp else 18.dp))

                    MediaProgress(
                        fraction = fraction,
                        position = position,
                        duration = duration,
                        modifier = Modifier.fillMaxWidth(),
                    )

                    Spacer(Modifier.height(if (compact) 12.dp else 18.dp))

                    TransportControls(
                        playing = media?.playing == true,
                        enabled = hasMedia,
                        mainSize = if (compact) 58 else 72,
                        sideSize = if (compact) 46 else 56,
                        onCommand = vm::mediaCommand,
                    )

                    if (!compact) {
                        Spacer(Modifier.height(18.dp))
                        VolumeBar(onDown = { vm.volumeStep("down") }, onUp = { vm.volumeStep("up") })
                    }

                    Spacer(Modifier.height(if (compact) 10.dp else 14.dp))

                    SourceLine(media = media, hasMedia = hasMedia)
                }
            }
        } else {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 22.dp, vertical = 8.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Artwork(
                    artUrl = artUrl,
                    corner = 30.dp,
                    iconSize = 64.dp,
                    modifier = Modifier
                        .widthIn(max = 380.dp)
                        .fillMaxWidth(0.88f)
                        .aspectRatio(1f),
                )

                Spacer(Modifier.height(24.dp))

                TrackDetails(
                    media = media,
                    align = TextAlign.Center,
                    showAlbum = true,
                )

                Spacer(Modifier.height(24.dp))

                MediaProgress(
                    fraction = fraction,
                    position = position,
                    duration = duration,
                    modifier = Modifier
                        .widthIn(max = 420.dp)
                        .fillMaxWidth(),
                )

                Spacer(Modifier.height(18.dp))

                TransportControls(
                    playing = media?.playing == true,
                    enabled = hasMedia,
                    mainSize = 78,
                    sideSize = 56,
                    onCommand = vm::mediaCommand,
                )

                Spacer(Modifier.height(26.dp))

                VolumeBar(onDown = { vm.volumeStep("down") }, onUp = { vm.volumeStep("up") })

                Spacer(Modifier.height(10.dp))

                SourceLine(media = media, hasMedia = hasMedia)

                Spacer(Modifier.height(12.dp))
            }
        }
    }
}

@Composable
private fun Artwork(
    artUrl: String?,
    corner: Dp,
    iconSize: Dp,
    modifier: Modifier = Modifier,
) {
    val shape = RoundedCornerShape(corner)
    Box(
        modifier = modifier
            .clip(shape)
            .background(Brush.linearGradient(listOf(CardRaised, Ink)))
            .border(1.dp, Stroke, shape),
        contentAlignment = Alignment.Center,
    ) {
        if (artUrl != null) {
            AsyncImage(
                model = ImageRequest.Builder(LocalContext.current)
                    .data(artUrl)
                    .crossfade(true)
                    .build(),
                contentDescription = "Albüm kapağı",
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        } else {
            Icon(
                Icons.Rounded.MusicNote,
                contentDescription = null,
                tint = TextMuted.copy(alpha = 0.7f),
                modifier = Modifier.size(iconSize),
            )
        }
    }
}

@Composable
private fun TrackDetails(
    media: MediaSnapshot?,
    align: TextAlign,
    showAlbum: Boolean,
) {
    val centered = align == TextAlign.Center
    Column(
        horizontalAlignment = if (centered) Alignment.CenterHorizontally else Alignment.Start,
    ) {
        Text(
            media?.title?.takeIf { it.isNotBlank() } ?: "Şimdi çalan yok",
            style = MaterialTheme.typography.headlineMedium,
            color = TextPrimary,
            textAlign = align,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.height(6.dp))
        Text(
            media?.artist?.takeIf { it.isNotBlank() } ?: "Bilinmeyen sanatçı",
            style = MaterialTheme.typography.bodyLarge,
            color = TextMuted,
            textAlign = align,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (showAlbum && media != null && media.album.isNotBlank()) {
            Spacer(Modifier.height(3.dp))
            Text(
                media.album,
                style = MaterialTheme.typography.bodyMedium,
                color = TextMuted.copy(alpha = 0.8f),
                textAlign = align,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun MediaProgress(
    fraction: Float,
    position: Long,
    duration: Long,
    modifier: Modifier = Modifier,
) {
    Column(modifier) {
        ProgressLine(fraction, Blue, height = 5)
        Spacer(Modifier.height(7.dp))
        Row(Modifier.fillMaxWidth()) {
            Text(formatDuration(position), style = MaterialTheme.typography.labelSmall, color = TextMuted)
            Spacer(Modifier.weight(1f))
            Text(formatDuration(duration), style = MaterialTheme.typography.labelSmall, color = TextMuted)
        }
    }
}

@Composable
private fun TransportControls(
    playing: Boolean,
    enabled: Boolean,
    mainSize: Int,
    sideSize: Int,
    onCommand: (String) -> Unit,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(18.dp),
    ) {
        RoundControl(
            icon = Icons.Rounded.SkipPrevious,
            description = "Önceki",
            size = sideSize,
            enabled = enabled,
            onClick = { onCommand("previous") },
        )

        Box(
            modifier = Modifier
                .size(mainSize.dp)
                .clip(CircleShape)
                .background(if (enabled) Blue else Blue.copy(alpha = 0.3f)),
            contentAlignment = Alignment.Center,
        ) {
            IconButton(
                onClick = { onCommand("play_pause") },
                enabled = enabled,
                modifier = Modifier.fillMaxSize(),
            ) {
                Icon(
                    if (playing) Icons.Rounded.Pause else Icons.Rounded.PlayArrow,
                    contentDescription = if (playing) "Duraklat" else "Oynat",
                    tint = Color.White,
                    modifier = Modifier.size((mainSize * 0.46f).dp),
                )
            }
        }

        RoundControl(
            icon = Icons.Rounded.SkipNext,
            description = "Sonraki",
            size = sideSize,
            enabled = enabled,
            onClick = { onCommand("next") },
        )
    }
}

@Composable
private fun VolumeBar(onDown: () -> Unit, onUp: () -> Unit) {
    Row(
        modifier = Modifier
            .widthIn(max = 420.dp)
            .fillMaxWidth()
            .clip(RoundedCornerShape(20.dp))
            .background(CardRaised.copy(alpha = 0.6f))
            .border(1.dp, Stroke, RoundedCornerShape(20.dp))
            .padding(horizontal = 10.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        IconButton(onClick = onDown) {
            Icon(Icons.Rounded.VolumeDown, "Sesi azalt", tint = TextMuted)
        }
        Text(
            "PC ses seviyesi",
            style = MaterialTheme.typography.bodyMedium,
            color = TextMuted,
        )
        IconButton(onClick = onUp) {
            Icon(Icons.Rounded.VolumeUp, "Sesi artır", tint = TextMuted)
        }
    }
}

@Composable
private fun SourceLine(media: MediaSnapshot?, hasMedia: Boolean) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(
            Modifier
                .size(6.dp)
                .clip(CircleShape)
                .background(if (hasMedia) Cyan else TextMuted.copy(alpha = 0.4f)),
        )
        Text(
            if (hasMedia) (media?.app?.ifBlank { "Medya" } ?: "Medya") else "Kaynak bekleniyor",
            style = MaterialTheme.typography.labelSmall,
            color = TextMuted,
            maxLines = 1,
        )
    }
}

@Composable
private fun RoundControl(
    icon: ImageVector,
    description: String,
    size: Int,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    Box(
        modifier = Modifier
            .size(size.dp)
            .clip(CircleShape)
            .background(CardRaised)
            .border(1.dp, Stroke, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        IconButton(onClick = onClick, enabled = enabled) {
            Icon(
                icon,
                contentDescription = description,
                tint = if (enabled) TextPrimary else TextMuted.copy(alpha = 0.4f),
                modifier = Modifier.size((size * 0.45f).dp),
            )
        }
    }
}
