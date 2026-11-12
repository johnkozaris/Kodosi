package com.kodosi.mobile.ui

import androidx.annotation.StringRes
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.constrainHeight
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

enum class AppSymbol {
    TERMINAL,
    PROJECTS,
    FRIENDS,
    SETTINGS,
    DOSI,
    ADD,
    BACK,
    MORE,
    SEND,
    CANVAS,
    CLOSE,
    COPY,
    FOLDER,
    KEYBOARD,
    COMPOSE,
    REFRESH,
    STOP,
    CHECK,
    TASKS,
    REPOSITORY,
    CHAT,
    INFO,
    FILE,
    BROWSER,
    LOCK,
    FORWARD,
    COMPUTER,
    PHONE,
    KEY,
    APPEARANCE,
    SUN,
    LANGUAGE,
    NOTIFICATIONS,
    TEXT_SIZE,
    SHEET,
    PRESENTATION,
    CHEVRONS,
    STACK,
    SEARCH,
    TEXT,
    HASH,
    BRACES,
    LIST,
    CODE,
    DIFF,
    PICTURE,
    QR_CODE,
    HIDE_KEYBOARD,
    ARROW_LEFT,
    ARROW_UP,
    ARROW_DOWN,
    ARROW_RIGHT
}

@Composable
fun AppIcon(
    symbol: AppSymbol,
    modifier: Modifier = Modifier,
    description: String? = null,
    tint: Color = KodosiTheme.colors.inkMuted
) {
    val size = 16.dp
    Icon(
        painter = remember(symbol) { SymbolPainter(symbol) },
        contentDescription = description,
        tint = tint,
        modifier = modifier.size(size)
    )
}

private class SymbolPainter(private val symbol: AppSymbol) : androidx.compose.ui.graphics.painter.Painter() {
    override val intrinsicSize = Size(24f, 24f)

    override fun androidx.compose.ui.graphics.drawscope.DrawScope.onDraw() {
        val scale = size.width / 24f
        val stroke = Stroke(1.7f * scale, cap = StrokeCap.Round)
        fun path(vararg points: Float) {
            val path = Path()
            points.asList().chunked(2).forEachIndexed { index, pair ->
                if (index ==
                    0
                ) {
                    path.moveTo(pair[0] * scale, pair[1] * scale)
                } else {
                    path.lineTo(pair[0] * scale, pair[1] * scale)
                }
            }
            drawPath(path, Color.Black, style = stroke)
        }
        fun rect(x: Float, y: Float, w: Float, h: Float, radius: Float = 2f) = drawRoundRect(
            Color.Black,
            Offset(x * scale, y * scale),
            Size(w * scale, h * scale),
            CornerRadius(radius * scale),
            style = stroke
        )
        fun circle(x: Float, y: Float, radius: Float) = drawCircle(
            Color.Black,
            radius * scale,
            Offset(
                x * scale,
                y * scale
            ),
            style = stroke
        )
        when (symbol) {
            AppSymbol.TERMINAL -> {
                rect(2f, 4f, 20f, 16f)
                path(6f, 9f, 9f, 12f, 6f, 15f)
                path(12f, 15f, 17f, 15f)
            }

            AppSymbol.PROJECTS -> {
                rect(3f, 3f, 7f, 7f)
                rect(14f, 3f, 7f, 7f)
                rect(3f, 14f, 7f, 7f)
                rect(14f, 14f, 7f, 7f)
            }

            AppSymbol.FRIENDS -> {
                circle(9f, 8f, 3f)
                path(3f, 20f, 3f, 17f, 5f, 14f, 13f, 14f, 15f, 17f, 15f, 20f)
                path(16f, 5f, 19f, 5f, 21f, 8f, 19f, 11f)
                path(18f, 14f, 21f, 16f, 21f, 20f)
            }

            AppSymbol.SETTINGS -> {
                path(4f, 6f, 20f, 6f)
                path(4f, 12f, 20f, 12f)
                path(4f, 18f, 20f, 18f)
                circle(9f, 6f, 2f)
                circle(16f, 12f, 2f)
                circle(8f, 18f, 2f)
            }

            AppSymbol.DOSI -> {
                rect(3f, 4f, 18f, 16f, 5f)
                rect(7f, 9f, 3f, 6f, 0.6f)
                rect(14f, 9f, 3f, 6f, 0.6f)
            }

            AppSymbol.ADD -> {
                path(12f, 4f, 12f, 20f)
                path(4f, 12f, 20f, 12f)
            }

            AppSymbol.BACK -> path(14f, 5f, 7f, 12f, 14f, 19f)

            AppSymbol.FORWARD -> path(10f, 5f, 17f, 12f, 10f, 19f)

            AppSymbol.MORE -> {
                circle(5f, 12f, 1f)
                circle(12f, 12f, 1f)
                circle(19f, 12f, 1f)
            }

            AppSymbol.SEND -> {
                path(5f, 13f, 12f, 6f, 19f, 13f)
                path(12f, 6f, 12f, 21f)
            }

            AppSymbol.CANVAS -> {
                rect(2f, 4f, 20f, 16f)
                path(10f, 4f, 10f, 20f)
                path(13f, 16f, 16f, 12f, 20f, 16f)
            }

            AppSymbol.CLOSE -> {
                path(6f, 6f, 18f, 18f)
                path(18f, 6f, 6f, 18f)
            }

            AppSymbol.COPY -> {
                rect(8f, 8f, 12f, 13f)
                path(15f, 4f, 4f, 4f, 4f, 16f)
            }

            AppSymbol.FOLDER -> path(3f, 6f, 10f, 6f, 12f, 9f, 21f, 9f, 21f, 20f, 3f, 20f, 3f, 6f)

            AppSymbol.KEYBOARD -> {
                rect(2f, 5f, 20f, 14f)
                path(6f, 9f, 6f, 10f)
                path(10f, 9f, 10f, 10f)
                path(14f, 9f, 14f, 10f)
                path(18f, 9f, 18f, 10f)
                path(7f, 15f, 17f, 15f)
            }

            AppSymbol.COMPOSE -> {
                rect(3f, 4f, 16f, 17f)
                path(8f, 15f, 10f, 10f, 19f, 2f, 22f, 5f, 13f, 14f, 8f, 15f)
            }

            AppSymbol.REFRESH -> {
                path(20f, 10f, 19f, 6f, 15f, 3f, 9f, 3f, 5f, 6f, 3f, 11f, 4f, 17f, 9f, 21f, 15f, 21f, 19f, 18f)
                path(20f, 3f, 20f, 10f, 13f, 10f)
            }

            AppSymbol.STOP -> rect(5f, 5f, 14f, 14f)

            AppSymbol.CHECK -> path(4f, 12f, 9f, 17f, 20f, 6f)

            AppSymbol.TASKS -> {
                rect(4f, 3f, 16f, 18f)
                path(8f, 8f, 10f, 10f, 15f, 6f)
                path(8f, 15f, 16f, 15f)
            }

            AppSymbol.REPOSITORY -> {
                path(5f, 3f, 5f, 21f, 18f, 21f, 18f, 3f, 5f, 3f)
                path(5f, 17f, 18f, 17f)
                path(9f, 7f, 14f, 7f)
                path(9f, 11f, 14f, 11f)
            }

            AppSymbol.CHAT -> {
                rect(3f, 4f, 17f, 12f, 3f)
                path(7f, 16f, 7f, 21f, 12f, 16f)
                path(7f, 9f, 16f, 9f)
            }

            AppSymbol.INFO -> {
                circle(12f, 12f, 9f)
                path(12f, 11f, 12f, 17f)
                circle(12f, 7f, 0.5f)
            }

            AppSymbol.FILE -> {
                path(5f, 2f, 14f, 2f, 20f, 8f, 20f, 22f, 5f, 22f, 5f, 2f)
                path(14f, 2f, 14f, 8f, 20f, 8f)
            }

            AppSymbol.BROWSER -> {
                circle(12f, 12f, 9f)
                path(3f, 12f, 21f, 12f)
                path(12f, 3f, 8f, 8f, 8f, 16f, 12f, 21f, 16f, 16f, 16f, 8f, 12f, 3f)
            }

            AppSymbol.LOCK -> {
                rect(5f, 10f, 14f, 11f)
                path(8f, 10f, 8f, 6f, 10f, 3f, 14f, 3f, 16f, 6f, 16f, 10f)
                path(12f, 14f, 12f, 17f)
            }

            AppSymbol.COMPUTER -> {
                rect(4f, 3f, 16f, 13f, 1f)
                path(4f, 16f, 1f, 20f, 23f, 20f, 20f, 16f)
            }

            AppSymbol.PHONE -> {
                rect(6f, 2f, 12f, 20f, 2.5f)
                path(10f, 18f, 14f, 18f)
            }

            AppSymbol.KEY -> {
                circle(8f, 8f, 5f)
                path(12f, 12f, 21f, 21f, 23f, 19f, 20f, 16f, 18f, 18f)
            }

            AppSymbol.APPEARANCE -> {
                circle(12f, 12f, 9f)
                val half = Path().apply {
                    moveTo(12f * scale, 3f * scale)
                    arcTo(
                        androidx.compose.ui.geometry.Rect(3f * scale, 3f * scale, 21f * scale, 21f * scale),
                        -90f,
                        180f,
                        false
                    )
                    close()
                }
                drawPath(half, Color.Black)
            }

            AppSymbol.SUN -> {
                circle(12f, 12f, 4f)
                path(12f, 1f, 12f, 4f)
                path(12f, 20f, 12f, 23f)
                path(1f, 12f, 4f, 12f)
                path(20f, 12f, 23f, 12f)
                path(4f, 4f, 6f, 6f)
                path(18f, 18f, 20f, 20f)
                path(4f, 20f, 6f, 18f)
                path(18f, 6f, 20f, 4f)
            }

            AppSymbol.LANGUAGE -> {
                path(2f, 5f, 14f, 5f)
                path(8f, 2f, 8f, 5f)
                path(12f, 5f, 10f, 11f, 3f, 17f)
                path(5f, 8f, 9f, 13f, 13f, 15f)
                path(13f, 22f, 18f, 10f, 23f, 22f)
                path(15f, 18f, 21f, 18f)
            }

            AppSymbol.NOTIFICATIONS -> {
                path(4f, 17f, 6f, 14f, 6f, 8f, 8f, 4f, 12f, 3f, 16f, 4f, 18f, 8f, 18f, 14f, 20f, 17f, 4f, 17f)
                path(9f, 21f, 12f, 22f, 15f, 21f)
            }

            AppSymbol.TEXT_SIZE -> {
                path(2f, 6f, 12f, 6f)
                path(7f, 6f, 7f, 20f)
                path(14f, 11f, 22f, 11f)
                path(18f, 11f, 18f, 20f)
            }

            AppSymbol.SHEET -> {
                rect(3f, 3f, 18f, 18f)
                path(3f, 9f, 21f, 9f)
                path(3f, 15f, 21f, 15f)
                path(9f, 3f, 9f, 21f)
                path(15f, 3f, 15f, 21f)
            }

            AppSymbol.PRESENTATION -> {
                rect(2f, 3f, 20f, 14f)
                path(12f, 17f, 12f, 22f)
                path(7f, 22f, 12f, 19f, 17f, 22f)
                path(8f, 12f, 11f, 8f, 15f, 11f, 18f, 6f)
            }

            AppSymbol.CHEVRONS -> {
                path(8f, 9f, 12f, 5f, 16f, 9f)
                path(8f, 15f, 12f, 19f, 16f, 15f)
            }

            AppSymbol.STACK -> {
                rect(3f, 9f, 18f, 12f)
                path(5f, 5f, 19f, 5f)
                path(7f, 2f, 17f, 2f)
            }

            AppSymbol.SEARCH -> {
                circle(10f, 10f, 7f)
                path(15f, 15f, 22f, 22f)
            }

            AppSymbol.TEXT -> {
                path(3f, 5f, 21f, 5f)
                path(3f, 10f, 17f, 10f)
                path(3f, 15f, 21f, 15f)
                path(3f, 20f, 13f, 20f)
            }

            AppSymbol.HASH -> {
                path(9f, 3f, 5f, 21f)
                path(18f, 3f, 14f, 21f)
                path(3f, 9f, 22f, 9f)
                path(2f, 16f, 21f, 16f)
            }

            AppSymbol.BRACES -> {
                path(9f, 3f, 6f, 3f, 6f, 9f, 3f, 12f, 6f, 15f, 6f, 21f, 9f, 21f)
                path(15f, 3f, 18f, 3f, 18f, 9f, 21f, 12f, 18f, 15f, 18f, 21f, 15f, 21f)
            }

            AppSymbol.LIST -> {
                circle(3f, 6f, 0.8f)
                circle(3f, 12f, 0.8f)
                circle(3f, 18f, 0.8f)
                path(8f, 6f, 21f, 6f)
                path(11f, 12f, 21f, 12f)
                path(8f, 18f, 21f, 18f)
            }

            AppSymbol.CODE -> {
                path(7f, 6f, 2f, 12f, 7f, 18f)
                path(17f, 6f, 22f, 12f, 17f, 18f)
                path(14f, 3f, 10f, 21f)
            }

            AppSymbol.DIFF -> {
                path(6f, 3f, 6f, 11f)
                path(2f, 7f, 10f, 7f)
                path(15f, 17f, 23f, 17f)
                path(17f, 2f, 7f, 22f)
            }

            AppSymbol.PICTURE -> {
                rect(2f, 3f, 20f, 18f)
                circle(16f, 8f, 2f)
                path(3f, 18f, 9f, 11f, 14f, 16f, 17f, 13f, 21f, 18f)
            }

            AppSymbol.QR_CODE -> {
                rect(3f, 3f, 7f, 7f, 1.5f)
                rect(14f, 3f, 7f, 7f, 1.5f)
                rect(3f, 14f, 7f, 7f, 1.5f)
                path(14f, 14f, 17f, 14f)
                path(20f, 14f, 21f, 14f)
                path(14f, 17f, 14f, 21f)
                path(17f, 17f, 21f, 17f)
                path(18f, 21f, 21f, 21f)
            }

            AppSymbol.HIDE_KEYBOARD -> {
                rect(3f, 2f, 18f, 12f)
                path(7f, 6f, 7f, 7f)
                path(12f, 6f, 12f, 7f)
                path(17f, 6f, 17f, 7f)
                path(8f, 10f, 16f, 10f)
                path(8f, 18f, 12f, 22f, 16f, 18f)
            }

            AppSymbol.ARROW_LEFT -> {
                path(9f, 5f, 2f, 12f, 9f, 19f)
                path(2f, 12f, 22f, 12f)
            }

            AppSymbol.ARROW_UP -> {
                path(5f, 9f, 12f, 2f, 19f, 9f)
                path(12f, 2f, 12f, 22f)
            }

            AppSymbol.ARROW_DOWN -> {
                path(5f, 15f, 12f, 22f, 19f, 15f)
                path(12f, 2f, 12f, 22f)
            }

            AppSymbol.ARROW_RIGHT -> {
                path(15f, 5f, 22f, 12f, 15f, 19f)
                path(2f, 12f, 22f, 12f)
            }
        }
    }
}

@Composable
fun ActionIcon(
    symbol: AppSymbol,
    @StringRes label: Int,
    onClick: () -> Unit,
    enabled: Boolean = true,
    active: Boolean = false,
    open: Boolean = false,
    face: Dp = 32.dp
) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovered by interaction.collectIsHoveredAsState()
    val focused by interaction.collectIsFocusedAsState()
    val scale by animateFloatAsState(
        if (pressed) 0.9f else 1f,
        if (reducedMotion()) snap() else spring(dampingRatio = 0.8f, stiffness = 620f),
        label = "icon press"
    )
    Box(
        Modifier.size(maxOf(48.dp, face)).clickable(
            interaction,
            indication = null,
            enabled = enabled,
            role = Role.Button,
            onClick = onClick
        ),
        contentAlignment = Alignment.Center
    ) {
        val shape = RoundedCornerShape(face * 0.28f)
        Box(
            Modifier.size(face).graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
                .then(
                    if (active || open) {
                        Modifier.raised(colors, shape, fill = if (active) colors.accentSoft else colors.lifted)
                    } else {
                        Modifier.background(
                            colors.ink.copy(
                                alpha = if (pressed) {
                                    0.13f
                                } else if (hovered || focused) {
                                    0.08f
                                } else {
                                    0f
                                }
                            ),
                            shape
                        )
                    }
                )
                .border(1.5.dp, if (focused) colors.accent.copy(alpha = 0.55f) else Color.Transparent, shape),
            contentAlignment = Alignment.Center
        ) {
            AppIcon(
                symbol,
                Modifier.size(if (face <= 24.dp) 11.dp else 14.dp),
                description = stringResource(label),
                tint = if (!enabled) {
                    colors.inkFaint
                } else if (active) {
                    colors.accentStrong
                } else if (open || hovered ||
                    pressed
                ) {
                    colors.ink
                } else {
                    colors.inkMuted
                }
            )
        }
    }
}

@Composable
fun PageHeader(
    title: String,
    back: (() -> Unit)? = null,
    titleContent: (@Composable () -> Unit)? = null,
    actions: @Composable () -> Unit = {}
) {
    val colors = KodosiTheme.colors
    if (back == null) {
        Row(
            Modifier.fillMaxWidth().statusBarsPadding().padding(start = 16.dp, end = 12.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                title,
                style = KodosiType.large,
                color = colors.ink,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).padding(top = 24.dp, bottom = 14.dp).semantics { heading() }
            )
            actions()
        }
        return
    }
    Row(
        Modifier.fillMaxWidth().statusBarsPadding().defaultMinSize(minHeight = 56.dp).padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        ActionIcon(AppSymbol.BACK, R.string.back, back)
        Box(Modifier.weight(1f).padding(start = 12.dp)) {
            if (titleContent != null) {
                titleContent()
            } else {
                Text(
                    title,
                    style = KodosiType.headline,
                    color = colors.ink,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.semantics { heading() }
                )
            }
        }
        actions()
    }
}

@Composable
fun SectionTitle(text: String) {
    Text(
        text,
        style = KodosiType.headline,
        color = KodosiTheme.colors.ink,
        modifier = Modifier.padding(top = 22.dp, bottom = 10.dp).semantics { heading() }
    )
}

@Composable
fun EmptyState(
    title: String,
    detail: String? = null,
    symbol: AppSymbol = AppSymbol.PROJECTS,
    action: @Composable () -> Unit = {}
) {
    val colors = KodosiTheme.colors
    Column(
        Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(14.dp)
    ) {
        AppIcon(symbol, modifier = Modifier.size(36.dp), tint = colors.accent)
        Text(title, style = KodosiType.headline, color = colors.ink, textAlign = TextAlign.Center)
        if (detail !=
            null
        ) {
            Text(detail, style = KodosiType.callout, color = colors.inkMuted, textAlign = TextAlign.Center)
        }
        action()
    }
}

@Composable
fun PersonAvatar(name: String, id: String = name, size: androidx.compose.ui.unit.Dp = 28.dp, own: Boolean = false) {
    val colors = KodosiTheme.colors
    val color = if (own) colors.accent else personTint(id)
    val initials = remember(name) {
        name.split(" ").filter {
            it.isNotBlank()
        }.take(2).mapNotNull { it.firstOrNull() }.joinToString("").uppercase().ifEmpty { "·" }
    }
    Box(
        Modifier.size(size).background(
            Brush.linearGradient(
                listOf(
                    lerp(color, Color.White, 0.18f),
                    color,
                    lerp(color, Color.Black, 0.16f)
                )
            ),
            CircleShape
        ),
        contentAlignment = Alignment.Center
    ) {
        Text(
            initials,
            style = KodosiType.headline.copy(
                fontSize = with(androidx.compose.ui.platform.LocalDensity.current) {
                    (size * 0.38f).toSp()
                }
            ),
            color = if (own) colors.onAccent else Color.White
        )
    }
}

@Composable
fun ProjectMark(id: String, size: Dp = 38.dp) {
    val colors = KodosiTheme.colors
    val tint = projectTint(id)
    val seed = identityHash(id)
    val shape = RoundedCornerShape(size * 0.3f)
    Canvas(
        Modifier.size(
            size
        ).shadow(size * 0.1f, shape, ambientColor = colors.shadow, spotColor = colors.shadow).background(
            Brush.linearGradient(
                listOf(
                    lerp(tint, Color.White, 0.16f),
                    lerp(tint, Color.Black, 0.08f),
                    lerp(tint, Color.Black, 0.34f)
                )
            ),
            shape
        )
    ) {
        val gap = this.size.width * 0.07f
        val inset = this.size.width * 0.2f
        val tile = (this.size.width - inset * 2 - gap) / 2
        for (y in 0..1) {
            for (x in 0..1) {
                val lit = ((seed shr (y * 2 + x + 3)) and 1uL) == 1uL || y * 2 + x == (seed % 4uL).toInt()
                drawRoundRect(
                    Color.White.copy(alpha = if (lit) 0.92f else 0.28f),
                    Offset(inset + x * (tile + gap), inset + y * (tile + gap)),
                    Size(tile, tile),
                    CornerRadius(tile * 0.28f)
                )
            }
        }
    }
}

@Composable
fun ConfirmDialog(
    title: String,
    message: String,
    action: String,
    dismiss: () -> Unit,
    confirm: () -> Unit,
    destructive: Boolean = false
) {
    KodosiDialog(
        onDismissRequest = dismiss,
        title = { Text(title) },
        text = { Text(message, Modifier.verticalScroll(rememberScrollState())) },
        confirmButton = {
            KodosiButton(action, onClick = confirm, kind = if (destructive) ButtonKind.DANGER else ButtonKind.PRIMARY)
        },
        dismissButton = { KodosiTextButton(onClick = dismiss) { Text(stringResource(R.string.cancel)) } }
    )
}

@Composable
fun TextEntryDialog(
    title: String,
    label: String,
    initial: String = "",
    action: String = stringResource(R.string.save),
    allowBlank: Boolean = false,
    dismiss: () -> Unit,
    submit: suspend (String) -> Unit
) {
    var text by remember { mutableStateOf(initial) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    KodosiDialog(
        onDismissRequest = { if (!busy) dismiss() },
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                KodosiField(text, {
                    text = it
                }, label = label, modifier = Modifier.fillMaxWidth(), enabled = !busy)
                error?.let { ErrorNote(it) }
                if (busy) ProgressCaption(stringResource(R.string.working))
            }
        },
        confirmButton = {
            KodosiButton(enabled = !busy && (allowBlank || text.isNotBlank()), onClick = {
                error = null
                busy = true
                scope.launch {
                    try {
                        submit(text)
                        dismiss()
                    } catch (failure: CancellationException) {
                        throw failure
                    } catch (failure: Exception) {
                        error = failure.localizedMessage
                    } finally {
                        busy = false
                    }
                }
            }) { Text(action) }
        },
        dismissButton = {
            KodosiTextButton(onClick = dismiss, enabled = !busy) { Text(stringResource(R.string.cancel)) }
        }
    )
}

@Composable
fun SelectableWords(text: String, modifier: Modifier = Modifier) {
    val links = LocalLinkOpener.current
    val colors = KodosiTheme.colors
    val content = remember(text, colors.accent, links) {
        buildAnnotatedString {
            append(text)
            LinkPolicy.inText(text).forEach { link ->
                addLink(
                    LinkAnnotation.Clickable(
                        link.address,
                        TextLinkStyles(
                            style = SpanStyle(color = colors.accent, textDecoration = TextDecoration.Underline)
                        )
                    ) { links.open(link.address) },
                    link.range.first,
                    link.range.last + 1
                )
            }
        }
    }
    SelectionContainer {
        Text(content, style = KodosiType.body, color = colors.ink, modifier = modifier)
    }
}

@Composable
fun ListLine(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    leading: (@Composable () -> Unit)? = null,
    titleColor: Color = KodosiTheme.colors.ink,
    trailing: @Composable () -> Unit = {}
) {
    val colors = KodosiTheme.colors
    val stacked = LocalDensity.current.fontScale >= 1.5f
    val hasSubtitle = !subtitle.isNullOrBlank()
    Layout(
        modifier = modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 8.dp),
        content = {
            Box(contentAlignment = Alignment.Center) { leading?.invoke() }
            Text(title, style = KodosiType.body, color = titleColor)
            if (hasSubtitle) Text(subtitle, style = KodosiType.footnote, color = colors.inkMuted)
            androidx.compose.foundation.layout.FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                trailing()
            }
        }
    ) { items, constraints ->
        val loose = constraints.copy(minWidth = 0, minHeight = 0, maxHeight = Constraints.Infinity)
        val mark = items[0].measure(loose)
        val markSpace = if (leading == null) 0 else maxOf(28.dp.roundToPx(), mark.width) + 12.dp.roundToPx()
        val bodyWidth = (constraints.maxWidth - markSpace).coerceAtLeast(0)
        val accessory = items.last().measure(loose.copy(maxWidth = bodyWidth))
        val space = if (accessory.width > 0) 12.dp.roundToPx() else 0
        val inline = !stacked && items[1].maxIntrinsicWidth(Constraints.Infinity) + accessory.width + space <= bodyWidth
        val wordsWidth = if (inline) (bodyWidth - accessory.width - space).coerceAtLeast(0) else bodyWidth
        val words = items[1].measure(loose.copy(maxWidth = wordsWidth))
        val detail = if (hasSubtitle) items[2].measure(loose.copy(maxWidth = wordsWidth)) else null
        val wordsHeight = words.height + (detail?.let { it.height + 2.dp.roundToPx() } ?: 0)
        val accessorySpace = if (!inline && accessory.height > 0) 10.dp.roundToPx() else 0
        val height = maxOf(
            36.dp.roundToPx(),
            mark.height,
            if (inline) maxOf(wordsHeight, accessory.height) else wordsHeight + accessorySpace + accessory.height
        )
        layout(constraints.maxWidth, constraints.constrainHeight(height)) {
            val top = if (inline) (height - wordsHeight) / 2 else 0
            mark.placeRelative(0, if (inline) (height - mark.height) / 2 else 0)
            words.placeRelative(markSpace, top)
            detail?.placeRelative(markSpace, top + words.height + 2.dp.roundToPx())
            accessory.placeRelative(
                if (inline) constraints.maxWidth - accessory.width else markSpace,
                if (inline) (height - accessory.height) / 2 else wordsHeight + accessorySpace
            )
        }
    }
}
