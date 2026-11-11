package com.kodosi.mobile.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalTonalElevationEnabled
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.MotionDurationScale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle

/** The roles of the Kodosi palette; plain values are dark and the light set is their pair. */
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
    val accentHover: Color,
    val accentStrong: Color,
    val accentSoft: Color,
    val onAccent: Color,
    val glowOrange: Color,
    val ready: Color,
    val caution: Color,
    val danger: Color,
    val dangerSoft: Color,
    val onDanger: Color,
    val highlight: Color,
    val lowlight: Color,
    val shadow: Color
) {
    val sheet: Color get() = androidx.compose.ui.graphics.lerp(raised, surface, 0.45f)
}

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
    accentHover = Color(0xFFE69B74),
    accentStrong = Color(0xFFEAA47E),
    accentSoft = Color(0xFF3E281C),
    onAccent = Color(0xFF160E0A),
    glowOrange = Color(0xFFF08A4B),
    ready = Color(0xFF7DBB99),
    caution = Color(0xFFDDB866),
    danger = Color(0xFFE77A75),
    dangerSoft = Color(0xFF3F211F),
    onDanger = Color(0xFF1A0C0B),
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
    accentHover = Color(0xFFC5672F),
    accentStrong = Color(0xFF9A4516),
    accentSoft = Color(0xFFF4DEC8),
    onAccent = Color(0xFFFFFAF3),
    glowOrange = Color(0xFFE8793A),
    ready = Color(0xFF2F7D54),
    caution = Color(0xFF8A6A1C),
    danger = Color(0xFFB23A32),
    dangerSoft = Color(0xFFF4D9D4),
    onDanger = Color(0xFFFFFAF3),
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
    val control = 13.dp
    val sheet = 22.dp
    val tile = 12.dp
}

/** The text styles of Kodosi, which follow the person's font size. */
object KodosiType {
    val large =
        TextStyle(fontSize = 34.sp, lineHeight = 40.sp, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.6).sp)
    val title =
        TextStyle(fontSize = 22.sp, lineHeight = 28.sp, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.4).sp)
    val headline =
        TextStyle(fontSize = 17.sp, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.2).sp)
    val subhead =
        TextStyle(fontSize = 15.sp, lineHeight = 20.sp, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.08).sp)
    val callout = TextStyle(fontSize = 16.sp, lineHeight = 22.sp, letterSpacing = (-0.08).sp)
    val body = TextStyle(fontSize = 17.sp, lineHeight = 23.sp, letterSpacing = (-0.08).sp)
    val footnote = TextStyle(fontSize = 13.sp, lineHeight = 18.sp)
    val caption = TextStyle(fontSize = 12.sp, lineHeight = 16.sp, fontWeight = FontWeight.Medium)
    val mono = TextStyle(
        fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
        fontSize = 13.sp,
        lineHeight = 18.sp
    )
}

private val LocalColors = staticCompositionLocalOf { DarkColors }
internal val LocalReducedMotion = staticCompositionLocalOf { false }

object KodosiTheme {
    val colors: KodosiColors
        @Composable get() = LocalColors.current
}

@Composable
fun KodosiTheme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val colors = if (dark) DarkColors else LightColors
    val inverse = if (dark) LightColors else DarkColors
    val motion = rememberCoroutineScope().coroutineContext[MotionDurationScale]
    val lifecycle by LocalLifecycleOwner.current.lifecycle.currentStateFlow.collectAsStateWithLifecycle(
        minActiveState = Lifecycle.State.CREATED
    )
    val reduced = motion?.scaleFactor == 0f || !lifecycle.isAtLeast(Lifecycle.State.STARTED)
    val scheme = remember(colors) {
        ColorScheme(
            primary = colors.accent,
            onPrimary = colors.onAccent,
            primaryContainer = colors.accentSoft,
            onPrimaryContainer = colors.ink,
            inversePrimary = inverse.accent,
            secondary = colors.inkMuted,
            onSecondary = colors.surface,
            secondaryContainer = colors.accentSoft,
            onSecondaryContainer = colors.ink,
            tertiary = colors.inkMuted,
            onTertiary = colors.surface,
            tertiaryContainer = colors.raised,
            onTertiaryContainer = colors.ink,
            background = colors.surface,
            onBackground = colors.ink,
            surface = colors.surface,
            onSurface = colors.ink,
            surfaceVariant = colors.well,
            onSurfaceVariant = colors.inkMuted,
            surfaceTint = colors.accent,
            inverseSurface = inverse.surface,
            inverseOnSurface = inverse.ink,
            error = colors.danger,
            onError = colors.onDanger,
            errorContainer = colors.dangerSoft,
            onErrorContainer = colors.danger,
            outline = colors.hairline,
            outlineVariant = colors.hairline,
            scrim = colors.shadow,
            surfaceBright = colors.lifted,
            surfaceDim = colors.ground,
            surfaceContainer = colors.raised,
            surfaceContainerHigh = colors.raised,
            surfaceContainerHighest = colors.lifted,
            surfaceContainerLow = colors.surface,
            surfaceContainerLowest = colors.ground,
            primaryFixed = LightColors.accentSoft,
            primaryFixedDim = LightColors.well,
            onPrimaryFixed = LightColors.ink,
            onPrimaryFixedVariant = LightColors.inkMuted,
            secondaryFixed = LightColors.raised,
            secondaryFixedDim = LightColors.ground,
            onSecondaryFixed = LightColors.ink,
            onSecondaryFixedVariant = LightColors.inkMuted,
            tertiaryFixed = LightColors.ground,
            tertiaryFixedDim = LightColors.well,
            onTertiaryFixed = LightColors.ink,
            onTertiaryFixedVariant = LightColors.inkMuted
        )
    }
    MaterialTheme(
        colorScheme = scheme,
        shapes = Shapes(
            extraSmall = RoundedCornerShape(Radius.xs),
            small = RoundedCornerShape(Radius.sm),
            medium = RoundedCornerShape(Radius.md),
            large = RoundedCornerShape(Radius.lg),
            extraLarge = RoundedCornerShape(Radius.sheet)
        ),
        typography = Typography(
            displayLarge = KodosiType.large,
            displayMedium = KodosiType.large,
            displaySmall = KodosiType.large,
            headlineLarge = KodosiType.large,
            headlineMedium = KodosiType.title,
            headlineSmall = KodosiType.title,
            titleLarge = KodosiType.title,
            titleMedium = KodosiType.headline,
            titleSmall = KodosiType.subhead,
            bodyLarge = KodosiType.body,
            bodyMedium = KodosiType.callout,
            bodySmall = KodosiType.footnote,
            labelLarge = KodosiType.subhead,
            labelMedium = KodosiType.caption,
            labelSmall = KodosiType.caption
        )
    ) {
        CompositionLocalProvider(
            LocalColors provides colors,
            LocalContentColor provides colors.ink,
            LocalReducedMotion provides reduced,
            LocalTonalElevationEnabled provides false,
            content = content
        )
    }
}
