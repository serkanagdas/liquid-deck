package com.liquiddeck.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.MusicNote
import androidx.compose.material.icons.rounded.Pause
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.compose.AsyncImage
import coil.request.ImageRequest
import com.liquiddeck.app.data.Connection
import com.liquiddeck.app.data.GpuMetrics
import com.liquiddeck.app.data.MediaSnapshot
import com.liquiddeck.app.data.Metrics
import com.liquiddeck.app.ui.theme.Cyan
import com.liquiddeck.app.ui.theme.Crimson
import com.liquiddeck.app.ui.theme.Ink
import com.liquiddeck.app.ui.theme.Lime
import com.liquiddeck.app.ui.theme.Orange
import com.liquiddeck.app.ui.theme.Purple
import com.liquiddeck.app.ui.theme.Red
import com.liquiddeck.app.ui.theme.Stroke
import com.liquiddeck.app.ui.theme.TextMuted
import com.liquiddeck.app.ui.theme.TextPrimary
import com.liquiddeck.app.ui.theme.Yellow
import java.util.Locale
import kotlin.math.roundToInt

/**
 * Sistem page, laid out like the web dashboard: one hero row with two identical
 * hardware cards (CPU and GPU), then one slim network strip.  Landscape is the
 * first-class orientation; portrait falls back to the same cards stacked.
 *
 * Both hero cards share the exact same anatomy — label + priority, subtitle,
 * two equal readings (Sıcaklık / Kullanım) and a bottom detail line — so the
 * CPU and GPU numbers always line up.  Per-core, fan and clock details move
 * into a tap-through sheet, exactly like the web panel's detail dialog.
 */
@Composable
fun SystemPage(
    ui: UiState,
    onOpenMedia: () -> Unit,
    onMediaCommand: (String) -> Unit,
    artUrl: (Long) -> String,
) {
    var detail by remember { mutableStateOf<SystemDetail?>(null) }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val landscape = maxWidth > maxHeight && maxWidth >= 560.dp
        if (landscape) {
            LandscapeSystem(
                ui = ui,
                showNowPlaying = maxHeight >= 460.dp,
                showCardBottom = maxHeight >= 240.dp,
                showCardExtra = maxHeight >= 460.dp,
                onOpenMedia = onOpenMedia,
                onMediaCommand = onMediaCommand,
                artUrl = artUrl,
                onDetail = { detail = it },
            )
        } else {
            PortraitSystem(
                ui = ui,
                onOpenMedia = onOpenMedia,
                onMediaCommand = onMediaCommand,
                artUrl = artUrl,
                onDetail = { detail = it },
            )
        }
    }

    detail?.let { target ->
        SystemDetailSheet(
            target = target,
            metrics = ui.metrics,
            version = ui.serverVersion,
            onDismiss = { detail = null },
        )
    }
}

// ---------------------------------------------------------------------------
// Layouts
// ---------------------------------------------------------------------------

@Composable
private fun LandscapeSystem(
    ui: UiState,
    showNowPlaying: Boolean,
    showCardBottom: Boolean,
    showCardExtra: Boolean,
    onOpenMedia: () -> Unit,
    onMediaCommand: (String) -> Unit,
    artUrl: (Long) -> String,
    onDetail: (SystemDetail) -> Unit,
) {
    val metrics = ui.metrics

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(start = 18.dp, end = 18.dp, top = 2.dp, bottom = 8.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (ui.connection is Connection.Offline) {
            OfflineBanner(ui.connection, hasData = metrics != null)
        }

        NetworkStrip(metrics)

        if (showNowPlaying) {
            NowPlayingStrip(
                media = ui.media,
                artUrl = artUrl,
                onOpen = onOpenMedia,
                onCommand = onMediaCommand,
            )
        }

        BoxWithConstraints(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth(),
        ) {
            val readingSize = readingSizeFor(maxHeight.value)
            Row(
                modifier = Modifier.fillMaxSize(),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                CpuHardwareCard(
                    metrics = metrics,
                    readingSize = readingSize,
                    showBottom = showCardBottom,
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxHeight(),
                    onDetail = { onDetail(SystemDetail.Cpu) },
                )
                GpuHardwareCard(
                    metrics = metrics,
                    readingSize = readingSize,
                    showBottom = showCardBottom,
                    showExtra = showCardExtra,
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxHeight(),
                    onDetail = { onDetail(SystemDetail.Gpu) },
                )
            }
        }

    }
}

@Composable
private fun PortraitSystem(
    ui: UiState,
    onOpenMedia: () -> Unit,
    onMediaCommand: (String) -> Unit,
    artUrl: (Long) -> String,
    onDetail: (SystemDetail) -> Unit,
) {
    val metrics = ui.metrics

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 18.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (ui.connection is Connection.Offline) {
            item { OfflineBanner(ui.connection, hasData = metrics != null) }
        }

        item { NetworkStrip(metrics) }

        item {
            NowPlayingStrip(
                media = ui.media,
                artUrl = artUrl,
                onOpen = onOpenMedia,
                onCommand = onMediaCommand,
            )
        }

        item {
            BoxWithConstraints(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(196.dp),
            ) {
                CpuHardwareCard(
                    metrics = metrics,
                    readingSize = readingSizeFor(maxHeight.value),
                    showBottom = true,
                    modifier = Modifier.fillMaxSize(),
                    onDetail = { onDetail(SystemDetail.Cpu) },
                )
            }
        }

        item {
            BoxWithConstraints(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(206.dp),
            ) {
                GpuHardwareCard(
                    metrics = metrics,
                    readingSize = readingSizeFor(maxHeight.value),
                    showBottom = true,
                    showExtra = true,
                    modifier = Modifier.fillMaxSize(),
                    onDetail = { onDetail(SystemDetail.Gpu) },
                )
            }
        }

        item { HostCard(metrics, ui.serverVersion) }
    }
}

private fun readingSizeFor(cardHeightDp: Float): TextUnit =
    (cardHeightDp * 0.31f).coerceIn(30f, 84f).sp

// ---------------------------------------------------------------------------
// Hero cards — one anatomy, two payloads
// ---------------------------------------------------------------------------

private data class Reading(
    val label: String,
    val value: String,
    val unit: String,
    val color: Color,
    val note: String? = null,
)

@Composable
private fun HardwareCard(
    modifier: Modifier,
    accent: Color,
    title: String,
    brand: String?,
    priority: String,
    subtitle: String,
    primary: Reading,
    usagePct: Float?,
    fanPct: Float?,
    readingSize: TextUnit,
    onDetail: () -> Unit,
    bottom: (@Composable () -> Unit)? = null,
) {
    GlassCard(
        modifier = modifier,
        accent = accent,
        onClick = onDetail,
        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = title,
                style = MaterialTheme.typography.labelSmall,
                color = accent,
            )
            if (!brand.isNullOrBlank()) {
                Spacer(Modifier.width(7.dp))
                Text(
                    text = brand,
                    fontSize = 10.sp,
                    color = TextMuted,
                    maxLines = 1,
                )
            }
            Spacer(Modifier.weight(1f))
            Text(
                text = priority,
                fontSize = 10.sp,
                color = TextMuted.copy(alpha = 0.45f),
            )
        }

        Spacer(Modifier.height(2.dp))

        Text(
            text = subtitle,
            fontSize = 12.sp,
            lineHeight = 15.sp,
            color = TextMuted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )

        Row(
            modifier = Modifier
                .weight(1f, fill = true)
                .fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            ReadingBlock(primary, readingSize, Modifier.weight(0.92f))
            BoxWithConstraints(
                modifier = Modifier
                    .weight(1.08f)
                    .fillMaxHeight(),
            ) {
                // Kullanım and Fan are the same dial at the same size; the pair
                // is sized from the card's own height, so both hero cards stay
                // identical at every orientation.  The 30 dp accounts for the
                // two captions and the small gaps between the rings.
                val ring = ((maxHeight.value - 30f) / 2f).coerceIn(28f, 74f).dp
                Column(
                    modifier = Modifier.fillMaxSize(),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    GaugeCaption("Kullanım")
                    Spacer(Modifier.height(2.dp))
                    GaugeRing(
                        percent = usagePct,
                        color = gaugeColor(usagePct),
                        size = ring,
                        strokeWidth = 3.dp,
                    )
                    Spacer(Modifier.height(5.dp))
                    GaugeCaption("Fan")
                    Spacer(Modifier.height(2.dp))
                    GaugeRing(
                        percent = fanPct,
                        color = gaugeColor(fanPct),
                        size = ring,
                        strokeWidth = 3.dp,
                    )
                }
            }
        }

        if (bottom != null) {
            Spacer(Modifier.height(4.dp))
            bottom()
        }
    }
}


@Composable
private fun ReadingBlock(reading: Reading, size: TextUnit, modifier: Modifier) {
    Column(modifier) {
        Text(
            text = reading.label,
            fontSize = 11.sp,
            color = TextMuted,
            maxLines = 1,
        )
        Spacer(Modifier.height(3.dp))
        ReadingValue(
            value = reading.value,
            unit = reading.unit,
            color = reading.color,
            size = size,
        )
        // Always reserve the note line so the two readings of every card, and
        // both cards, keep identical vertical rhythm.
        Box(Modifier.height(13.dp)) {
            reading.note?.let { note ->
                Text(
                    text = note,
                    fontSize = 10.sp,
                    color = reading.color,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun ReadingValue(value: String, unit: String, color: Color, size: TextUnit) {
    val fontSize = (size.value * if (value.length >= 3) 0.78f else 1f).sp
    Row(verticalAlignment = Alignment.Bottom) {
        Text(
            text = value,
            style = TextStyle(
                fontFamily = FontFamily.SansSerif,
                fontWeight = FontWeight.Medium,
                fontSize = fontSize,
                letterSpacing = (fontSize.value * -0.04f).sp,
                fontFeatureSettings = "tnum",
            ),
            color = color,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Clip,
        )
        Text(
            text = unit,
            style = TextStyle(
                fontFamily = FontFamily.SansSerif,
                fontWeight = FontWeight.Medium,
                fontSize = (fontSize.value * 0.30f).sp,
                fontFeatureSettings = "tnum",
            ),
            color = color,
            modifier = Modifier.padding(start = 2.dp, bottom = (fontSize.value * 0.13f).dp),
        )
    }
}

@Composable
private fun CpuHardwareCard(
    metrics: Metrics?,
    readingSize: TextUnit,
    showBottom: Boolean,
    modifier: Modifier,
    onDetail: () -> Unit,
) {
    val temp = metrics?.temp

    HardwareCard(
        modifier = modifier,
        accent = Crimson,
        title = "CPU",
        brand = null,
        priority = "01",
        subtitle = when {
            metrics == null -> "Veri bekleniyor"
            metrics.cpuCores > 0 -> "${metrics.cpuCores} iş parçacığı"
            else -> "İşlemci"
        },
        primary = Reading(
            label = "Sıcaklık",
            value = temp?.roundToInt()?.toString() ?: "—",
            unit = "°C",
            color = heatColor(temp),
        ),
        usagePct = metrics?.cpu,
        fanPct = metrics?.cpuFanPct?.toFloat(),
        readingSize = readingSize,
        onDetail = onDetail,
        bottom = if (showBottom) {
            { CpuBottomLine(metrics) }
        } else {
            null
        },
    )
}

@Composable
private fun GpuHardwareCard(
    metrics: Metrics?,
    readingSize: TextUnit,
    showBottom: Boolean,
    showExtra: Boolean,
    modifier: Modifier,
    onDetail: () -> Unit,
) {
    val gpu = metrics?.gpu
    val supported = gpu?.supported == true

    HardwareCard(
        modifier = modifier,
        accent = Lime,
        title = "GPU",
        brand = gpuBrand(gpu?.name),
        priority = "02",
        subtitle = when {
            gpu?.name?.isNotBlank() == true -> gpu.name
            metrics == null -> "Veri bekleniyor"
            else -> "GPU bulunamadı"
        },
        primary = Reading(
            label = "Sıcaklık",
            value = gpu?.temp?.toString() ?: "—",
            unit = "°C",
            color = heatColor(gpu?.temp?.toFloat()),
        ),
        usagePct = if (supported) gpu?.usage?.toFloat() else null,
        fanPct = if (supported) gpu?.fanPct?.toFloat() else null,
        readingSize = readingSize,
        onDetail = onDetail,
        bottom = if (showBottom) {
            { GpuBottomLine(gpu, showExtra) }
        } else {
            null
        },
    )
}

@Composable
private fun CpuBottomLine(metrics: Metrics?) {
    Text(
        text = metrics?.tempSource?.takeIf { it.isNotBlank() } ?: "CPU sensörü okunamıyor",
        fontSize = 10.sp,
        color = TextMuted.copy(alpha = 0.55f),
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

@Composable
private fun GpuBottomLine(gpu: GpuMetrics?, showExtra: Boolean) {
    Column {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = "VRAM",
                fontSize = 10.sp,
                color = TextMuted.copy(alpha = 0.6f),
            )
            Spacer(Modifier.width(6.dp))
            val memory = gpu?.let { card ->
                if (card.memoryUsed != null && card.memoryTotal != null) {
                    "${formatBytes(card.memoryUsed)} / ${formatBytes(card.memoryTotal)}"
                } else {
                    null
                }
            }
            Text(
                text = memory ?: "Veri yok",
                fontSize = 10.5.sp,
                color = if (memory == null) TextMuted else TextPrimary,
                maxLines = 1,
            )
            Spacer(Modifier.weight(1f))
        }
        if (showExtra) {
            Spacer(Modifier.height(3.dp))
            Text(
                text = gpuDetailLine(gpu),
                fontSize = 10.sp,
                color = TextMuted.copy(alpha = 0.55f),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

private fun gpuDetailLine(gpu: GpuMetrics?): String {
    if (gpu == null) return "Çekirdek — MHz · Bellek — MHz · Güç — W"
    val parts = buildList {
        gpu.clockMhz?.let { add("Çekirdek $it MHz") }
        gpu.memClockMhz?.let { add("Bellek $it MHz") }
        gpu.powerWatts?.let { watts ->
            val limit = gpu.powerLimitWatts
            add(
                if (limit != null && limit > 0) {
                    String.format(Locale.US, "Güç %.0f/%.0f W", watts, limit)
                } else {
                    String.format(Locale.US, "Güç %.1f W", watts)
                },
            )
        }
    }
    return if (parts.isEmpty()) "Ayrıntı için dokun" else parts.joinToString(" · ")
}

// ---------------------------------------------------------------------------
// Network + RAM strip
// ---------------------------------------------------------------------------

@Composable
private fun NetworkStrip(metrics: Metrics?) {
    val down = rateParts(metrics?.netRx ?: 0.0)
    val up = rateParts(metrics?.netTx ?: 0.0)
    val ping = metrics?.ping?.takeIf { it.isFinite() && it >= 0f }

    GlassCard(contentPadding = PaddingValues(horizontal = 16.dp, vertical = 10.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            StripStat(
                label = "↓ İndirme",
                value = if (metrics == null) "—" else down.first,
                unit = if (metrics == null) "B/s" else down.second,
                color = Cyan,
                modifier = Modifier.weight(1f),
            )
            StripStat(
                label = "↑ Yükleme",
                value = if (metrics == null) "—" else up.first,
                unit = if (metrics == null) "B/s" else up.second,
                color = Purple,
                modifier = Modifier.weight(1f),
            )
            StripStat(
                label = "TCP Ping",
                value = ping?.let { if (it < 10f) String.format(Locale.US, "%.1f", it) else it.roundToInt().toString() } ?: "—",
                unit = "ms",
                color = TextPrimary,
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(12.dp))
            Box(
                Modifier
                    .width(1.dp)
                    .height(34.dp)
                    .background(Stroke),
            )
            Spacer(Modifier.width(14.dp))
            RamStat(metrics)
        }
    }
}

@Composable
private fun StripStat(
    label: String,
    value: String,
    unit: String,
    color: Color,
    modifier: Modifier,
) {
    Column(modifier = modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Text(
            text = label,
            fontSize = 9.5.sp,
            color = TextMuted,
            maxLines = 1,
        )
        Spacer(Modifier.height(2.dp))
        Row(verticalAlignment = Alignment.Bottom) {
            Text(
                text = value,
                fontSize = 18.sp,
                fontWeight = FontWeight.Medium,
                color = color,
                maxLines = 1,
                softWrap = false,
            )
            Text(
                text = unit,
                fontSize = 9.5.sp,
                color = TextMuted,
                modifier = Modifier.padding(start = 2.dp, bottom = 2.dp),
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun RamStat(metrics: Metrics?) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(
            text = "RAM",
            fontSize = 9.5.sp,
            color = TextMuted,
            maxLines = 1,
        )
        Spacer(Modifier.height(2.dp))
        Row(verticalAlignment = Alignment.Bottom) {
            Text(
                text = if (metrics == null) "—" else metrics.ramPct.roundToInt().toString(),
                fontSize = 18.sp,
                fontWeight = FontWeight.Medium,
                color = gaugeColor(metrics?.ramPct),
                maxLines = 1,
                softWrap = false,
            )
            Text(
                text = "%",
                fontSize = 9.5.sp,
                color = TextMuted,
                modifier = Modifier.padding(start = 2.dp, bottom = 2.dp),
            )
        }
        Text(
            text = if (metrics == null) "—" else memoryPair(metrics.ramUsed, metrics.ramTotal),
            fontSize = 9.sp,
            color = TextMuted,
            maxLines = 1,
        )
    }
}

// ---------------------------------------------------------------------------
// Now playing + offline
// ---------------------------------------------------------------------------

@Composable
private fun OfflineBanner(connection: Connection, hasData: Boolean) {
    val reason = (connection as? Connection.Offline)?.reason
    GlassCard(accent = Red, contentPadding = PaddingValues(horizontal = 16.dp, vertical = 10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                if (hasData) "Bağlantı koptu · son veriler" else "PC'ye ulaşılamıyor",
                style = MaterialTheme.typography.titleMedium,
                color = TextPrimary,
            )
            Spacer(Modifier.weight(1f))
            Text(
                reason ?: "Sunucu çalışıyor mu?",
                style = MaterialTheme.typography.bodyMedium,
                color = TextMuted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun NowPlayingStrip(
    media: MediaSnapshot?,
    artUrl: (Long) -> String,
    onOpen: () -> Unit,
    onCommand: (String) -> Unit,
) {
    GlassCard(onClick = onOpen, contentPadding = PaddingValues(horizontal = 14.dp, vertical = 10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                modifier = Modifier
                    .size(44.dp)
                    .clip(RoundedCornerShape(13.dp))
                    .background(Color.White.copy(alpha = 0.06f)),
                contentAlignment = Alignment.Center,
            ) {
                if (media != null && media.artVersion > 0) {
                    AsyncImage(
                        model = ImageRequest.Builder(LocalContext.current)
                            .data(artUrl(media.artVersion))
                            .crossfade(true)
                            .build(),
                        contentDescription = null,
                        contentScale = ContentScale.Crop,
                        modifier = Modifier.fillMaxSize(),
                    )
                } else {
                    Icon(
                        Icons.Rounded.MusicNote,
                        contentDescription = null,
                        tint = TextMuted,
                        modifier = Modifier.size(21.dp),
                    )
                }
            }

            Spacer(Modifier.size(12.dp))

            Column(Modifier.weight(1f)) {
                Text(
                    media?.title?.takeIf { it.isNotBlank() } ?: "Şimdi çalan yok",
                    style = MaterialTheme.typography.titleMedium,
                    color = TextPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(1.dp))
                Text(
                    media?.artist?.takeIf { it.isNotBlank() }
                        ?: "PC'de bir medya oynatıcı aç",
                    style = MaterialTheme.typography.bodyMedium,
                    color = TextMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }

            if (media != null && media.title.isNotBlank()) {
                IconButton(onClick = { onCommand("play_pause") }) {
                    Icon(
                        if (media.playing) Icons.Rounded.Pause else Icons.Rounded.PlayArrow,
                        contentDescription = if (media.playing) "Duraklat" else "Oynat",
                        tint = TextPrimary,
                    )
                }
            }

            Icon(
                Icons.Rounded.ChevronRight,
                contentDescription = null,
                tint = TextMuted,
                modifier = Modifier.size(20.dp),
            )
        }
    }
}

@Composable
private fun HostCard(metrics: Metrics?, version: String) {
    GlassCard {
        SectionLabel("Sistem")
        Spacer(Modifier.height(10.dp))
        KeyValueRow("PC", metrics?.host?.takeIf { it.isNotBlank() } ?: "—")
        Spacer(Modifier.height(7.dp))
        KeyValueRow("İşletim sistemi", metrics?.os?.takeIf { it.isNotBlank() } ?: "—")
        Spacer(Modifier.height(7.dp))
        KeyValueRow("Çalışma süresi", metrics?.let { formatUptime(it.uptime) } ?: "—")
        Spacer(Modifier.height(7.dp))
        KeyValueRow("Sunucu sürümü", version.ifBlank { "—" })
        metrics?.cpuPower?.let { power ->
            Spacer(Modifier.height(7.dp))
            KeyValueRow("CPU gücü", String.format(Locale.US, "%.1f W", power))
        }
    }
}

// ---------------------------------------------------------------------------
// Detail sheet (the web panel opens the same numbers in a dialog)
// ---------------------------------------------------------------------------

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SystemDetailSheet(
    target: SystemDetail,
    metrics: Metrics?,
    version: String,
    onDismiss: () -> Unit,
) {
    DeckSheet(
        onDismiss = onDismiss,
        accent = if (target == SystemDetail.Cpu) Crimson else Lime,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 22.dp)
                .padding(top = 20.dp, bottom = 26.dp),
        ) {
            when (target) {
                SystemDetail.Cpu -> CpuDetail(metrics, version)
                SystemDetail.Gpu -> GpuDetail(metrics)
            }
        }
    }
}

@Composable
private fun CpuDetail(metrics: Metrics?, version: String) {
    val temp = metrics?.temp

    DetailHeading(
        eyebrow = "İŞLEMCİ",
        accent = Crimson,
        title = "CPU ayrıntıları",
        subtitle = when {
            metrics == null -> "Veri bekleniyor"
            metrics.cpuCores > 0 -> "${metrics.cpuCores} iş parçacığı · ${metrics.host}"
            else -> metrics.host
        },
    )

    Spacer(Modifier.height(16.dp))

    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        StatBox(
            label = "Sıcaklık",
            value = temp?.roundToInt()?.toString() ?: "—",
            unit = "°C",
            color = heatColor(temp),
            modifier = Modifier.weight(1f),
        )
        StatBox(
            label = "Kullanım",
            value = if (metrics == null) "—" else metrics.cpu.roundToInt().toString(),
            unit = "%",
            color = gaugeColor(metrics?.cpu),
            modifier = Modifier.weight(1f),
        )
        StatBox(
            label = "Güç",
            value = metrics?.cpuPower?.let { String.format(Locale.US, "%.0f", it) } ?: "—",
            unit = "W",
            color = TextPrimary,
            modifier = Modifier.weight(1f),
        )
    }

    val fans = metrics?.cpuFans.orEmpty().filterNotNull()
    if (fans.isNotEmpty() || metrics?.cpuFanPct != null) {
        Spacer(Modifier.height(20.dp))
        DetailSectionTitle("Fanlar")
        Spacer(Modifier.height(8.dp))
        metrics?.cpuFanPct?.let { pct ->
            KeyValueRow("CPU fan PWM", "%$pct")
            Spacer(Modifier.height(7.dp))
        }
        fans.forEachIndexed { index, rpm ->
            KeyValueRow(fanLabel(index), if (rpm == 0) "0 RPM" else "$rpm RPM")
            Spacer(Modifier.height(7.dp))
        }
        metrics?.cpuChip?.takeIf { it.isNotBlank() }?.let { chip ->
            KeyValueRow("Denetleyici", chip)
        }
    }

    if (metrics != null && metrics.cpus.size > 1) {
        Spacer(Modifier.height(20.dp))
        DetailSectionTitle("İş parçacıkları")
        Spacer(Modifier.height(10.dp))
        ThreadGrid(metrics.cpus)
    }

    Spacer(Modifier.height(20.dp))
    HorizontalDivider(color = Stroke)
    Spacer(Modifier.height(14.dp))
    KeyValueRow("Sensör", metrics?.tempSource?.takeIf { it.isNotBlank() } ?: "—")
    Spacer(Modifier.height(7.dp))
    KeyValueRow("Çalışma süresi", metrics?.let { formatUptime(it.uptime) } ?: "—")
    Spacer(Modifier.height(7.dp))
    KeyValueRow(
        "Yük",
        metrics?.load?.takeIf { it.isNotEmpty() }
            ?.joinToString(" · ") { String.format(Locale.US, "%.2f", it) }
            ?: "—",
    )
    Spacer(Modifier.height(7.dp))
    KeyValueRow("Sunucu sürümü", version.ifBlank { "—" })
}

@Composable
private fun GpuDetail(metrics: Metrics?) {
    val gpu = metrics?.gpu
    val supported = gpu?.supported == true

    DetailHeading(
        eyebrow = "GRAFİK İŞLEMCİ",
        accent = Lime,
        title = "GPU ayrıntıları",
        subtitle = gpu?.name?.takeIf { it.isNotBlank() }
            ?: if (metrics == null) "Veri bekleniyor" else "GPU bulunamadı",
    )

    if (!supported) {
        Spacer(Modifier.height(14.dp))
        Text(
            text = gpu?.status ?: "Bu PC'de GPU telemetrisi yok.",
            style = MaterialTheme.typography.bodyMedium,
            color = TextMuted,
        )
        return
    }

    Spacer(Modifier.height(16.dp))

    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        StatBox(
            label = "Sıcaklık",
            value = gpu?.temp?.toString() ?: "—",
            unit = "°C",
            color = heatColor(gpu?.temp?.toFloat()),
            modifier = Modifier.weight(1f),
        )
        StatBox(
            label = "Kullanım",
            value = gpu?.usage?.toString() ?: "—",
            unit = "%",
            color = gaugeColor(gpu?.usage?.toFloat()),
            modifier = Modifier.weight(1f),
        )
        StatBox(
            label = "Fan",
            value = when {
                gpu?.fanRpm != null -> gpu.fanRpm.toString()
                gpu?.fanPct != null -> gpu.fanPct.toString()
                else -> "—"
            },
            unit = when {
                gpu?.fanRpm != null -> "RPM"
                gpu?.fanPct != null -> "%"
                else -> ""
            },
            color = TextPrimary,
            modifier = Modifier.weight(1f),
        )
    }

    val used = gpu?.memoryUsed
    val total = gpu?.memoryTotal
    if (used != null && total != null) {
        Spacer(Modifier.height(20.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            DetailSectionTitle("VRAM")
            Spacer(Modifier.weight(1f))
            Text(
                text = "${formatBytes(used)} / ${formatBytes(total)}",
                style = MaterialTheme.typography.bodyMedium,
                color = TextPrimary,
            )
        }
        Spacer(Modifier.height(8.dp))
        ProgressLine((gpu.memoryPct ?: 0f) / 100f, Lime)
    }

    Spacer(Modifier.height(20.dp))
    HorizontalDivider(color = Stroke)
    Spacer(Modifier.height(14.dp))

    KeyValueRow("Çekirdek", gpu?.clockMhz?.let { "$it MHz" } ?: "—")
    Spacer(Modifier.height(7.dp))
    KeyValueRow("Bellek", gpu?.memClockMhz?.let { "$it MHz" } ?: "—")
    Spacer(Modifier.height(7.dp))
    KeyValueRow(
        "Güç / Limit",
        gpu?.powerWatts?.let { watts ->
            val limit = gpu.powerLimitWatts
            if (limit != null && limit > 0) {
                String.format(Locale.US, "%.0f / %.0f W", watts, limit)
            } else {
                String.format(Locale.US, "%.1f W", watts)
            }
        } ?: "—",
    )
    Spacer(Modifier.height(7.dp))
    KeyValueRow("P-State", gpu?.pstate?.let { "P$it" } ?: "—")
    gpu?.status?.takeIf { it.isNotBlank() }?.let { status ->
        Spacer(Modifier.height(7.dp))
        KeyValueRow("Durum", status)
    }
}

@Composable
private fun DetailHeading(eyebrow: String, accent: Color, title: String, subtitle: String) {
    Text(
        text = eyebrow,
        style = MaterialTheme.typography.labelSmall,
        color = accent,
    )
    Spacer(Modifier.height(4.dp))
    Text(title, style = MaterialTheme.typography.headlineMedium)
    if (subtitle.isNotBlank()) {
        Spacer(Modifier.height(3.dp))
        Text(
            text = subtitle,
            style = MaterialTheme.typography.bodyMedium,
            color = TextMuted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun DetailSectionTitle(text: String) {
    SectionLabel(text, TextPrimary.copy(alpha = 0.75f))
}

@Composable
private fun StatBox(
    label: String,
    value: String,
    unit: String,
    color: Color,
    modifier: Modifier,
) {
    val shape = RoundedCornerShape(16.dp)
    Column(
        modifier = modifier
            .clip(shape)
            .background(Color.White.copy(alpha = 0.04f))
            .border(1.dp, Stroke, shape)
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        SectionLabel(label)
        Spacer(Modifier.height(4.dp))
        Row(verticalAlignment = Alignment.Bottom) {
            Text(
                text = value,
                style = TextStyle(
                    fontFamily = FontFamily.SansSerif,
                    fontWeight = FontWeight.Medium,
                    fontSize = 26.sp,
                    letterSpacing = (-0.5).sp,
                    fontFeatureSettings = "tnum",
                ),
                color = color,
                maxLines = 1,
                softWrap = false,
            )
            if (value != "—") {
                Text(
                    text = unit,
                    fontSize = 11.sp,
                    color = color.copy(alpha = 0.7f),
                    modifier = Modifier.padding(start = 2.dp, bottom = 3.dp),
                    maxLines = 1,
                )
            }
        }
    }
}

@Composable
private fun ThreadGrid(threads: List<Int>) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        threads.chunked(3).forEachIndexed { rowIndex, row ->
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                row.forEachIndexed { column, usage ->
                    val index = rowIndex * 3 + column
                    val value = usage.coerceIn(0, 100)
                    Column(Modifier.weight(1f)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                text = "${index + 1}",
                                fontSize = 10.sp,
                                color = TextMuted,
                            )
                            Spacer(Modifier.weight(1f))
                            Text(
                                text = "$value%",
                                fontSize = 11.sp,
                                color = TextPrimary,
                            )
                        }
                        Spacer(Modifier.height(3.dp))
                        ProgressLine(value / 100f, gaugeColor(value.toFloat()), height = 3)
                    }
                }
                repeat(3 - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

private sealed interface SystemDetail {
    data object Cpu : SystemDetail
    data object Gpu : SystemDetail
}

/** Shared CPU/GPU usage, fan duty and RAM scale: white → orange → red. */
private fun gaugeColor(pct: Float?): Color = when {
    pct == null -> TextMuted
    pct < 40f -> TextPrimary
    pct < 70f -> Orange
    else -> Red
}

/** Temperature only: up to 60 °C white, above 60–80 °C orange, above 80 °C red. */
private fun heatColor(temp: Float?): Color = when {
    temp == null -> TextMuted
    temp > 80f -> Red
    temp > 60f -> Orange
    else -> TextPrimary
}

private fun gpuBrand(name: String?): String? {
    val upper = name?.uppercase(Locale.US) ?: return null
    return when {
        "NVIDIA" in upper || "GEFORCE" in upper || "RTX" in upper -> "NVIDIA"
        "AMD" in upper || "RADEON" in upper -> "AMD"
        "INTEL" in upper || "ARC" in upper -> "INTEL"
        else -> null
    }
}

private fun fanLabel(index: Int): String = when (index) {
    0 -> "CPU fan"
    1 -> "Sistem fan 1"
    2 -> "Sistem fan 2"
    else -> "Fan ${index + 1}"
}

/** Splits "12.4 MB" into ("12.4", "MB/s") so the unit can stay small. */
private fun rateParts(bytesPerSecond: Double): Pair<String, String> {
    val text = formatBytes(bytesPerSecond.toLong())
    val split = text.lastIndexOf(' ')
    if (split <= 0) return text to "B/s"
    return text.substring(0, split) to "${text.substring(split + 1)}/s"
}

private fun memoryPair(used: Long, total: Long): String {
    if (total <= 0) return "—"
    val u = compactBytes(used)
    val t = compactBytes(total)
    return if (u.second == t.second) {
        "${u.first}/${t.first} ${t.second}"
    } else {
        "${u.first} ${u.second} / ${t.first} ${t.second}"
    }
}

private fun compactBytes(bytes: Long): Pair<String, String> {
    val units = arrayOf("B", "KB", "MB", "GB", "TB")
    var value = bytes.toDouble()
    var index = 0
    while (value >= 1024 && index < units.lastIndex) {
        value /= 1024
        index++
    }
    val text = if (index == 0 || value >= 100) {
        value.roundToInt().toString()
    } else {
        String.format(Locale.US, "%.1f", value).trimEnd('0').trimEnd('.')
    }
    return text to units[index]
}
