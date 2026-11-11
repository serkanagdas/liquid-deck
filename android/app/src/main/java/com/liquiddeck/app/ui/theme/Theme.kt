package com.liquiddeck.app.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

val Amoled = Color(0xFF000000)
val Ink = Color(0xFF0B0B0F)
val Card = Color(0xFF121217)
val CardRaised = Color(0xFF1A1A21)
val Stroke = Color(0x14FFFFFF)

val TextPrimary = Color(0xFFF5F5F7)
val TextMuted = Color(0xFF8E8E93)

val Blue = Color(0xFF0A84FF)
val Cyan = Color(0xFF64D2FF)
val Orange = Color(0xFFFF9F0A)
val Purple = Color(0xFFBF5AF2)
val Pink = Color(0xFFFF375F)
val Green = Color(0xFF30D158)
val Red = Color(0xFFFF453A)
val Yellow = Color(0xFFFFD60A)
val Slate = Color(0xFF8E8E93)
val Lime = Color(0xFF9DDC48)
val Lilac = Color(0xFFD4A4F5)

private val Scheme = darkColorScheme(
    primary = Blue,
    onPrimary = Color.White,
    secondary = Cyan,
    onSecondary = Color.Black,
    tertiary = Orange,
    background = Amoled,
    onBackground = TextPrimary,
    surface = Card,
    onSurface = TextPrimary,
    surfaceVariant = CardRaised,
    onSurfaceVariant = TextMuted,
    outline = Stroke,
    error = Red,
    onError = Color.White,
)

private val AppTypography = Typography(
    displayLarge = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Bold,
        fontSize = 66.sp,
        letterSpacing = (-2.5).sp,
    ),
    displayMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Bold,
        fontSize = 46.sp,
        letterSpacing = (-1.5).sp,
    ),
    headlineMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.SemiBold,
        fontSize = 24.sp,
        letterSpacing = (-0.4).sp,
    ),
    titleLarge = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.SemiBold,
        fontSize = 20.sp,
    ),
    titleMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Medium,
        fontSize = 16.sp,
    ),
    bodyLarge = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Normal,
        fontSize = 15.sp,
    ),
    bodyMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Normal,
        fontSize = 13.sp,
    ),
    labelLarge = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.SemiBold,
        fontSize = 13.sp,
    ),
    labelSmall = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Medium,
        fontSize = 10.5.sp,
        letterSpacing = 0.9.sp,
    ),
)

@Composable
fun LiquidDeckTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = Scheme,
        typography = AppTypography,
        content = content,
    )
}
