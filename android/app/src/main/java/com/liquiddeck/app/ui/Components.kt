package com.liquiddeck.app.ui

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.liquiddeck.app.data.Connection
import com.liquiddeck.app.ui.theme.Card
import com.liquiddeck.app.ui.theme.Green
import com.liquiddeck.app.ui.theme.Stroke
import com.liquiddeck.app.ui.theme.TextMuted
import com.liquiddeck.app.ui.theme.TextPrimary
import com.liquiddeck.app.ui.theme.Yellow
import kotlin.math.roundToInt

@Composable
fun GlassCard(
    modifier: Modifier = Modifier,
    accent: Color? = null,
    onClick: (() -> Unit)? = null,
    contentPadding: PaddingValues = PaddingValues(18.dp),
    content: @Composable ColumnScope.() -> Unit,
) {
    val shape = RoundedCornerShape(24.dp)
    Column(
        modifier = modifier
            .clip(shape)
            .background(accent?.copy(alpha = 0.10f) ?: Card)
            .border(1.dp, accent?.copy(alpha = 0.32f) ?: Stroke, shape)
            .then(onClick?.let { callback -> Modifier.clickable { callback() } } ?: Modifier)
            .padding(contentPadding),
        content = content,
    )
}

@Composable
fun SectionLabel(text: String, color: Color = TextMuted) {
    Text(
        text = text.uppercase(),
        style = MaterialTheme.typography.labelSmall,
        color = color,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

@Composable
fun StatusPill(connection: Connection, modifier: Modifier = Modifier) {
    val (label, color) = when (connection) {
        is Connection.Online -> "Bağlı" to Green
        is Connection.Connecting -> "Bağlanıyor" to Yellow
        is Connection.Offline -> "Çevrimdışı" to TextMuted
    }
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(50))
            .background(color.copy(alpha = 0.12f))
            .border(1.dp, color.copy(alpha = 0.3f), RoundedCornerShape(50))
            .padding(horizontal = 9.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(
            Modifier
                .size(6.dp)
                .clip(CircleShape)
                .background(color),
        )
        Text(
            text = label,
            style = MaterialTheme.typography.labelSmall,
            color = color,
            maxLines = 1,
        )
    }
}

@Composable
fun ProgressLine(
    progress: Float,
    accent: Color,
    modifier: Modifier = Modifier,
    height: Int = 6,
) {
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(height.dp)
            .clip(CircleShape)
            .background(Color.White.copy(alpha = 0.08f)),
    ) {
        Box(
            Modifier
                .fillMaxWidth(progress.coerceIn(0f, 1f))
                .fillMaxHeight()
                .clip(CircleShape)
                .background(accent),
        )
    }
}

/**
 * Tiny caption above a gauge: KULLANIM, FAN, KALAN …
 */
@Composable
fun GaugeCaption(text: String) {
    Text(
        text = text.uppercase(),
        fontSize = 8.5.sp,
        letterSpacing = 0.8.sp,
        color = TextMuted,
        maxLines = 1,
    )
}

/**
 * Minimal 270° speedometer ring: a plain number with a small % in the middle
 * and an analog arc around it.  [percent] is 0..100; null draws an empty track
 * with an em dash, so a sensor that is missing never fakes a reading.
 */
@Composable
fun GaugeRing(
    percent: Float?,
    color: Color,
    size: Dp,
    modifier: Modifier = Modifier,
    strokeWidth: Dp = 3.dp,
) {
    val target = percent?.coerceIn(0f, 100f) ?: 0f
    val sweep by animateFloatAsState(
        targetValue = target,
        animationSpec = tween(durationMillis = 650, easing = FastOutSlowInEasing),
        label = "gaugeSweep",
    )
    val valueSize = (size.value * 0.31f).coerceAtLeast(10f).sp
    val unitSize = (size.value * 0.15f).coerceAtLeast(7f).sp

    Box(modifier = modifier.size(size), contentAlignment = Alignment.Center) {
        Canvas(Modifier.fillMaxSize()) {
            val stroke = strokeWidth.toPx()
            val edge = stroke / 2f
            val square = this.size.minDimension - stroke
            val topLeft = Offset(
                x = (this.size.width - square) / 2f,
                y = (this.size.height - square) / 2f,
            )
            val arcSize = Size(square, square)
            drawArc(
                color = Stroke,
                startAngle = 135f,
                sweepAngle = 270f,
                useCenter = false,
                topLeft = topLeft,
                size = arcSize,
                style = Stroke(stroke, cap = StrokeCap.Round),
            )
            if (percent != null && sweep > 0.2f) {
                drawArc(
                    color = color,
                    startAngle = 135f,
                    sweepAngle = 270f * sweep / 100f,
                    useCenter = false,
                    topLeft = topLeft,
                    size = arcSize,
                    style = Stroke(stroke, cap = StrokeCap.Round),
                )
            }
        }

        if (percent == null) {
            Text(
                text = "—",
                style = TextStyle(
                    fontFamily = FontFamily.SansSerif,
                    fontWeight = FontWeight.Medium,
                    fontSize = valueSize,
                    fontFeatureSettings = "tnum",
                ),
                color = TextMuted,
                maxLines = 1,
                softWrap = false,
            )
        } else {
            Row(verticalAlignment = Alignment.Bottom) {
                Text(
                    text = sweep.roundToInt().toString(),
                    style = TextStyle(
                        fontFamily = FontFamily.SansSerif,
                        fontWeight = FontWeight.Medium,
                        fontSize = valueSize,
                        letterSpacing = (valueSize.value * -0.03f).sp,
                        fontFeatureSettings = "tnum",
                    ),
                    color = color,
                    maxLines = 1,
                    softWrap = false,
                )
                Text(
                    text = "%",
                    style = TextStyle(
                        fontFamily = FontFamily.SansSerif,
                        fontWeight = FontWeight.Medium,
                        fontSize = unitSize,
                        fontFeatureSettings = "tnum",
                    ),
                    color = color.copy(alpha = 0.75f),
                    modifier = Modifier.padding(start = 1.dp, bottom = 2.dp),
                    maxLines = 1,
                )
            }
        }
    }
}

@Composable
fun KeyValueRow(label: String, value: String, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = MaterialTheme.typography.bodyMedium, color = TextMuted)
        Spacer(Modifier.width(12.dp))
        Text(
            value,
            style = MaterialTheme.typography.bodyMedium,
            color = TextPrimary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
