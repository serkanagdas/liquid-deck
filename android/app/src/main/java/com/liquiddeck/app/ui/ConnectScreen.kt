package com.liquiddeck.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.liquiddeck.app.ui.theme.Amoled
import com.liquiddeck.app.ui.theme.Blue
import com.liquiddeck.app.ui.theme.Card
import com.liquiddeck.app.ui.theme.Cyan
import com.liquiddeck.app.ui.theme.Red
import com.liquiddeck.app.ui.theme.Stroke
import com.liquiddeck.app.ui.theme.TextMuted
import com.liquiddeck.app.ui.theme.TextPrimary

@Composable
fun ConnectScreen(ui: UiState, onConnect: (String, String) -> Unit) {
    var host by rememberSaveable(ui.host) { mutableStateOf(ui.host) }
    var token by rememberSaveable(ui.token) { mutableStateOf(ui.token) }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    listOf(Blue.copy(alpha = 0.22f), Amoled, Amoled),
                ),
            ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 26.dp, vertical = 34.dp),
        ) {
            Spacer(Modifier.height(26.dp))

            Box(
                modifier = Modifier
                    .size(76.dp)
                    .clip(RoundedCornerShape(24.dp))
                    .background(Brush.linearGradient(listOf(Cyan, Blue))),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    "LD",
                    fontSize = 27.sp,
                    fontWeight = FontWeight.Black,
                    color = Color.Black,
                )
            }

            Spacer(Modifier.height(24.dp))
            Text(
                "LIQUID DECK",
                style = MaterialTheme.typography.labelSmall,
                color = Cyan,
            )
            Spacer(Modifier.height(8.dp))
            Text(
                "PC'ndeki panoya bağlan",
                style = MaterialTheme.typography.headlineMedium,
                color = TextPrimary,
            )
            Spacer(Modifier.height(8.dp))
            Text(
                "Telemetri, medya ve makro tuşları ev ağındaki bilgisayarından gelir.",
                style = MaterialTheme.typography.bodyLarge,
                color = TextMuted,
            )

            Spacer(Modifier.height(28.dp))

            OutlinedTextField(
                value = host,
                onValueChange = { host = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Sunucu adresi") },
                placeholder = { Text("192.168.1.50:8787") },
                singleLine = true,
                shape = RoundedCornerShape(16.dp),
                keyboardOptions = KeyboardOptions(
                    keyboardType = KeyboardType.Uri,
                    imeAction = ImeAction.Next,
                ),
                colors = fieldColors(),
            )

            Spacer(Modifier.height(12.dp))

            OutlinedTextField(
                value = token,
                onValueChange = { token = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Token (opsiyonel)") },
                placeholder = { Text("Sunucu token'ı") },
                singleLine = true,
                shape = RoundedCornerShape(16.dp),
                keyboardOptions = KeyboardOptions(
                    keyboardType = KeyboardType.Password,
                    imeAction = ImeAction.Done,
                ),
                colors = fieldColors(),
            )

            val error = ui.error
            if (error != null) {
                Spacer(Modifier.height(14.dp))
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(14.dp))
                        .background(Red.copy(alpha = 0.12f))
                        .padding(horizontal = 14.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        error,
                        style = MaterialTheme.typography.bodyMedium,
                        color = Red,
                    )
                }
            }

            Spacer(Modifier.height(18.dp))

            Button(
                onClick = { onConnect(host, token) },
                enabled = !ui.connecting,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(54.dp),
                shape = RoundedCornerShape(18.dp),
                colors = ButtonDefaults.buttonColors(
                    containerColor = Blue,
                    contentColor = Color.White,
                    disabledContainerColor = Blue.copy(alpha = 0.4f),
                ),
            ) {
                if (ui.connecting) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(20.dp),
                        color = Color.White,
                        strokeWidth = 2.dp,
                    )
                    Spacer(Modifier.width(10.dp))
                    Text("Bağlanıyor…")
                } else {
                    Text("Bağlan", style = MaterialTheme.typography.labelLarge)
                }
            }

            Spacer(Modifier.height(28.dp))

            GlassCard {
                SectionLabel("Bağlantı kontrol listesi")
                Spacer(Modifier.height(12.dp))
                ChecklistRow("1", "PC'de liquid-deck çalışıyor olsun.")
                Spacer(Modifier.height(9.dp))
                ChecklistRow("2", "Telefon ve PC aynı Wi-Fi ağında olsun.")
                Spacer(Modifier.height(9.dp))
                ChecklistRow("3", "Adresi portla birlikte gir: 192.168.1.50:8787")
                Spacer(Modifier.height(9.dp))
                ChecklistRow("4", "Token kullandıysan aynı token'ı buraya yaz.")
            }

            Spacer(Modifier.height(18.dp))
            Text(
                "İpucu: PC'de `--lan` ile başlat, güvenlik duvarında 8787 portuna izin ver.",
                style = MaterialTheme.typography.bodyMedium,
                color = TextMuted,
            )
        }
    }
}

@Composable
private fun ChecklistRow(number: String, text: String) {
    Row(verticalAlignment = Alignment.Top) {
        Box(
            modifier = Modifier
                .size(20.dp)
                .clip(RoundedCornerShape(7.dp))
                .background(Cyan.copy(alpha = 0.14f)),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                number,
                style = MaterialTheme.typography.labelSmall,
                color = Cyan,
            )
        }
        Spacer(Modifier.width(10.dp))
        Text(
            text,
            style = MaterialTheme.typography.bodyMedium,
            color = TextPrimary,
        )
    }
}

@Composable
private fun fieldColors() = OutlinedTextFieldDefaults.colors(
    focusedTextColor = TextPrimary,
    unfocusedTextColor = TextPrimary,
    focusedBorderColor = Blue,
    unfocusedBorderColor = Stroke,
    focusedLabelColor = Blue,
    unfocusedLabelColor = TextMuted,
    cursorColor = Blue,
    focusedContainerColor = Card,
    unfocusedContainerColor = Card,
    focusedPlaceholderColor = TextMuted.copy(alpha = 0.6f),
    unfocusedPlaceholderColor = TextMuted.copy(alpha = 0.6f),
)
