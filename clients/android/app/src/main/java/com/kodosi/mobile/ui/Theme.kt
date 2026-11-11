package com.kodosi.mobile.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@Immutable
data class KodosiColors(
    val isDark: Boolean,
    val ground: Color,
    val surface: Color,
    val raised: Color,
    val lifted: Color,
    val well: Color,
    val terminal: Color,
    val band: Color,
    val hairline: Color,
    val ink: Color,
    val inkMuted: Color,
    val inkFaint: Color,
    val accent: Color,
    val accentSoft: Color,
    val onAccent: Color,
    val glowOrange: Color,
    val ready: Color,
    val caution: Color,
    val danger: Color,
    val highlight: Color,
    val lowlight: Color,
    val shadow: Color
)

val DarkColors = KodosiColors(
    isDark = true,
    ground = Color(0xFF110E0C),
    surface = Color(0xFF181411),
    raised = Color(0xFF251D19),
    lifted = Color(0xFF2E241F),
    well = Color(0xFF0C0A08),
    terminal = Color(0xFF0D0B09),
    band = Color(0xFF221A16),
    hairline = Color(0xFF3B2F29),
    ink = Color(0xFFF1E9E3),
    inkMuted = Color(0xFFB3A094),
    inkFaint = Color(0xFF8C7A6F),
    accent = Color(0xFFDB8A62),
    accentSoft = Color(0xFF3E281C),
    onAccent = Color(0xFF160E0A),
    glowOrange = Color(0xFFF08A4B),
    ready = Color(0xFF7DBB99),
    caution = Color(0xFFDDB866),
    danger = Color(0xFFE77A75),
    highlight = Color(0x17FFF1DC),
    lowlight = Color(0x4D000000),
    shadow = Color.Black
)

val LightColors = KodosiColors(
    isDark = false,
    ground = Color(0xFFF0E9DC),
    surface = Color(0xFFF7F2E8),
    raised = Color(0xFFFEFBF5),
    lifted = Color(0xFFFFFDF9),
    well = Color(0xFFE8E0D2),
    terminal = Color(0xFF0D0B09),
    band = Color(0xFF221A16),
    hairline = Color(0xFFD9CCBA),
    ink = Color(0xFF25160E),
    inkMuted = Color(0xFF5F4838),
    inkFaint = Color(0xFF8A7262),
    accent = Color(0xFFB85A26),
    accentSoft = Color(0xFFF4DEC8),
    onAccent = Color(0xFFFFFAF3),
    glowOrange = Color(0xFFE8793A),
    ready = Color(0xFF2F7D54),
    caution = Color(0xFF8A6A1C),
    danger = Color(0xFFB23A32),
    highlight = Color(0xF2FFFFFF),
    lowlight = Color(0x0F4F3017),
    shadow = Color(0xFF2E1F0F)
)

object Radius {
    val xs = 6.dp
    val sm = 8.dp
    val md = 10.dp
    val lg = 14.dp
    val xl = 18.dp
    val tile = 12.dp
}

object KodosiType {
    val large =
        TextStyle(fontSize = 32.sp, lineHeight = 38.sp, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.6).sp)
    val title =
        TextStyle(fontSize = 22.sp, lineHeight = 28.sp, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.4).sp)
    val headline =
        TextStyle(fontSize = 17.sp, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.2).sp)
    val subhead =
        TextStyle(fontSize = 15.sp, lineHeight = 20.sp, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.08).sp)
    val callout = TextStyle(fontSize = 16.sp, lineHeight = 22.sp, letterSpacing = (-0.08).sp)
    val body = TextStyle(fontSize = 16.sp, lineHeight = 22.sp, letterSpacing = (-0.08).sp)
    val footnote = TextStyle(fontSize = 13.sp, lineHeight = 18.sp)
    val caption = TextStyle(fontSize = 12.sp, lineHeight = 16.sp, fontWeight = FontWeight.Medium)
}

private val LocalColors = staticCompositionLocalOf { DarkColors }

object KodosiTheme {
    val colors: KodosiColors
        @Composable get() = LocalColors.current
}

@Composable
fun KodosiTheme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val colors = if (dark) DarkColors else LightColors
    val scheme = if (dark) {
        darkColorScheme(
            primary = colors.accent,
            onPrimary = colors.onAccent,
            background = colors.surface,
            onBackground = colors.ink,
            surface = colors.surface,
            onSurface = colors.ink,
            surfaceVariant = colors.well,
            onSurfaceVariant = colors.inkMuted,
            outline = colors.hairline,
            error = colors.danger
        )
    } else {
        lightColorScheme(
            primary = colors.accent,
            onPrimary = colors.onAccent,
            background = colors.surface,
            onBackground = colors.ink,
            surface = colors.surface,
            onSurface = colors.ink,
            surfaceVariant = colors.well,
            onSurfaceVariant = colors.inkMuted,
            outline = colors.hairline,
            error = colors.danger
        )
    }
    MaterialTheme(
        colorScheme = scheme,
        typography = Typography(
            bodyLarge = KodosiType.body,
            bodyMedium = KodosiType.callout,
            labelLarge = KodosiType.subhead
        )
    ) {
        CompositionLocalProvider(LocalColors provides colors, content = content)
    }
}
