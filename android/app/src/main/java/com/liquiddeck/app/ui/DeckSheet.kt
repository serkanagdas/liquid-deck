package com.liquiddeck.app.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.liquiddeck.app.ui.theme.Card
import com.liquiddeck.app.ui.theme.CardRaised
import com.liquiddeck.app.ui.theme.Cyan
import com.liquiddeck.app.ui.theme.Stroke
import com.liquiddeck.app.ui.theme.TextMuted
import com.liquiddeck.app.ui.theme.TextPrimary

/** A shared, bounded panel: matches the dashboard without blur or extra motion. */
@Composable
fun DeckSheet(
    onDismiss: () -> Unit,
    accent: Color = Cyan,
    content: @Composable ColumnScope.() -> Unit,
) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        BoxWithConstraints(
            Modifier.fillMaxWidth().imePadding().padding(horizontal = 18.dp, vertical = 16.dp),
            contentAlignment = Alignment.Center,
        ) {
            Surface(
                modifier = Modifier.widthIn(max = 700.dp).fillMaxWidth().heightIn(max = maxHeight),
                shape = RoundedCornerShape(28.dp),
                color = Card,
                contentColor = TextPrimary,
                border = BorderStroke(1.dp, Color.White.copy(alpha = 0.13f)),
                tonalElevation = 0.dp,
            ) {
                Column {
                    Row(
                        modifier = Modifier.fillMaxWidth()
                            .background(Brush.horizontalGradient(listOf(accent.copy(alpha = 0.09f), CardRaised)))
                            .padding(start = 22.dp, end = 6.dp, top = 2.dp, bottom = 2.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("LIQUID DECK", style = MaterialTheme.typography.labelSmall, color = accent)
                        Spacer(Modifier.weight(1f))
                        IconButton(onClick = onDismiss, modifier = Modifier.size(48.dp)) {
                            Icon(Icons.Rounded.Close, "Kapat", tint = TextMuted, modifier = Modifier.size(20.dp))
                        }
                    }
                    HorizontalDivider(color = Stroke)
                    // Callers own their scroll state; the header stays visible above it.
                    Box(Modifier.weight(1f, fill = false)) {
                        Column(content = content)
                    }
                }
            }
        }
    }
}
