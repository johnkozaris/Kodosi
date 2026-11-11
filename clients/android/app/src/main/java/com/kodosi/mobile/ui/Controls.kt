package com.kodosi.mobile.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
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
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.ProvideTextStyle
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R

enum class ButtonKind { PRIMARY, SECONDARY, TINTED, GHOST, DANGER }

enum class ButtonSize { SMALL, REGULAR, LARGE }

@Composable
fun KodosiButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: ButtonKind = ButtonKind.PRIMARY,
    size: ButtonSize = ButtonSize.REGULAR,
    enabled: Boolean = true
) {
    KodosiButton(onClick, modifier, kind, size, enabled) {
        Text(
            text,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center
        )
    }
}

@Composable
fun KodosiButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: ButtonKind = ButtonKind.PRIMARY,
    size: ButtonSize = ButtonSize.REGULAR,
    enabled: Boolean = true,
    content: @Composable RowScope.() -> Unit
) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovered by interaction.collectIsHoveredAsState()
    val focused by interaction.collectIsFocusedAsState()
    val lit = enabled && (hovered || focused)
    val shape = RoundedCornerShape(
        when (size) {
            ButtonSize.SMALL -> 10.dp
            ButtonSize.REGULAR -> Radius.control
            ButtonSize.LARGE -> 16.dp
        }
    )
    val (fill, ink) = when (kind) {
        ButtonKind.PRIMARY -> (if (lit) colors.accentHover else colors.accent) to colors.onAccent

        ButtonKind.SECONDARY -> lerp(
            colors.lifted,
            colors.ink,
            if (lit) {
                if (colors.isDark) 0.05f else 0.03f
            } else {
                0f
            }
        ) to colors.ink

        ButtonKind.TINTED -> lerp(colors.accentSoft, colors.accent, if (lit) 0.14f else 0f) to colors.accentStrong

        ButtonKind.GHOST -> Color.Transparent to if (lit || pressed) colors.ink else colors.inkMuted

        ButtonKind.DANGER -> colors.danger to colors.onDanger
    }
    val face = when {
        kind == ButtonKind.GHOST -> colors.ink.copy(
            alpha = if (pressed) {
                0.13f
            } else if (lit) {
                0.08f
            } else {
                0f
            }
        )

        !enabled -> colors.lifted

        pressed -> lerp(fill, Color.Black, 0.08f)

        else -> fill
    }
    val scale by animateFloatAsState(
        if (pressed) 0.97f else 1f,
        if (reducedMotion()) snap() else spring(dampingRatio = 0.8f, stiffness = 620f),
        label = "button press"
    )
    val quiet = !enabled || kind == ButtonKind.SECONDARY || kind == ButtonKind.TINTED
    val faceModifier = if (kind == ButtonKind.GHOST) {
        Modifier.background(face, shape)
    } else {
        Modifier.bezel(colors, shape, face, quiet)
    }
    CompositionLocalProvider(LocalContentColor provides if (enabled) ink else colors.inkFaint) {
        ProvideTextStyle(
            when (size) {
                ButtonSize.SMALL -> KodosiType.footnote
                ButtonSize.REGULAR -> KodosiType.subhead
                ButtonSize.LARGE -> KodosiType.headline
            }
        ) {
            Row(
                modifier
                    .clickable(
                        interactionSource = interaction,
                        indication = null,
                        enabled = enabled,
                        role = Role.Button,
                        onClick = onClick
                    )
                    .graphicsLayer {
                        scaleX = scale
                        scaleY = scale
                    }
                    .then(faceModifier)
                    .border(1.5.dp, if (focused) colors.accent.copy(alpha = 0.55f) else Color.Transparent, shape)
                    .defaultMinSize(
                        minHeight = when (size) {
                            ButtonSize.SMALL -> 34.dp
                            ButtonSize.REGULAR -> 44.dp
                            ButtonSize.LARGE -> 52.dp
                        }
                    )
                    .padding(
                        horizontal = when (size) {
                            ButtonSize.SMALL -> 10.dp
                            ButtonSize.REGULAR -> 18.dp
                            ButtonSize.LARGE -> 22.dp
                        },
                        vertical = 8.dp
                    ),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.Center,
                content = content
            )
        }
    }
}

@Composable
fun KodosiTextButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    content: @Composable RowScope.() -> Unit
) {
    KodosiButton(
        onClick,
        modifier,
        kind = ButtonKind.GHOST,
        size = ButtonSize.SMALL,
        enabled = enabled,
        content = content
    )
}

@Composable
fun KodosiSendKey(enabled: Boolean, working: Boolean, onClick: () -> Unit) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val focused by interaction.collectIsFocusedAsState()
    val reduced = reducedMotion()
    val scale by animateFloatAsState(
        if (pressed) 0.95f else 1f,
        if (reduced) snap() else spring(dampingRatio = 0.8f, stiffness = 620f),
        label = "send press"
    )
    Box(
        Modifier.size(48.dp).graphicsLayer {
            scaleX = scale
            scaleY = scale
        }
            .clip(CircleShape)
            .clickable(
                interaction,
                indication = null,
                enabled = enabled && !working,
                role = Role.Button,
                onClick = onClick
            ),
        contentAlignment = Alignment.Center
    ) {
        Box(
            Modifier.size(30.dp)
                .bezel(
                    colors,
                    CircleShape,
                    if (enabled) colors.accent else lerp(colors.lifted, colors.ink, 0.08f),
                    quiet = !enabled
                )
                .border(
                    1.5.dp,
                    if (focused || (working && reduced)) {
                        colors.accentStrong
                    } else {
                        Color.Transparent
                    },
                    CircleShape
                ),
            contentAlignment = Alignment.Center
        ) {
            AppIcon(
                AppSymbol.SEND,
                modifier = Modifier.size(14.dp),
                description = stringResource(R.string.send),
                tint = if (enabled) colors.onAccent else colors.inkFaint
            )
            if (working && !reduced) {
                KodosiSpinner(Modifier.matchParentSize())
            }
        }
    }
}

@Composable
fun KodosiField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    label: String? = null,
    placeholder: String? = null,
    enabled: Boolean = true,
    singleLine: Boolean = false,
    minLines: Int = 1,
    maxLines: Int = Int.MAX_VALUE,
    textStyle: TextStyle = KodosiType.body,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    compact: Boolean = false,
    leading: (@Composable () -> Unit)? = null
) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val shape = RoundedCornerShape(if (compact) Radius.sm else Radius.control)
    Column(modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (label != null) {
            Text(
                label,
                style = KodosiType.footnote,
                color = colors.inkMuted,
                modifier = Modifier.clearAndSetSemantics {
                }
            )
        }
        BasicTextField(
            value, onValueChange,
            enabled = enabled,
            singleLine = singleLine,
            minLines = minLines,
            maxLines = maxLines,
            textStyle = textStyle.copy(color = if (enabled) colors.ink else colors.inkFaint),
            cursorBrush = SolidColor(colors.accent),
            interactionSource = interaction,
            keyboardOptions = keyboardOptions,
            keyboardActions = keyboardActions,
            modifier = Modifier.fillMaxWidth().defaultMinSize(minHeight = if (compact) 34.dp else 48.dp)
                .well(colors, shape)
                .border(1.5.dp, if (focused) colors.accent.copy(alpha = 0.55f) else Color.Transparent, shape)
                .padding(horizontal = 12.dp, vertical = if (compact) 8.dp else 12.dp)
                .semantics { (label ?: placeholder)?.let { contentDescription = it } },
            decorationBox = { input ->
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp)
                ) {
                    leading?.invoke()
                    Box(Modifier.weight(1f)) {
                        if (value.isEmpty() &&
                            placeholder != null
                        ) {
                            Text(placeholder, style = textStyle, color = colors.inkMuted)
                        }
                        input()
                    }
                }
            }
        )
    }
}

@Composable
fun KodosiSwitch(checked: Boolean, onCheckedChange: (Boolean) -> Unit, label: String, enabled: Boolean = true) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val position by animateFloatAsState(
        if (checked) 1f else 0f,
        if (reducedMotion()) snap() else spring(dampingRatio = 0.8f, stiffness = 620f),
        label = "switch"
    )
    Box(
        Modifier.size(46.dp, 28.dp).semantics { contentDescription = label }
            .toggleable(
                checked,
                interaction,
                indication = null,
                enabled = enabled,
                role = Role.Switch,
                onValueChange = onCheckedChange
            ),
        contentAlignment = Alignment.Center
    ) {
        Box(
            Modifier.size(46.dp, 28.dp).graphicsLayer { alpha = if (enabled) 1f else 0.45f }
                .well(colors, CircleShape)
                .background(if (checked) colors.accent else Color.Transparent, CircleShape)
                .border(1.5.dp, if (focused) colors.accentStrong else Color.Transparent, CircleShape),
            contentAlignment = Alignment.CenterStart
        ) {
            Box(
                Modifier.offset { IntOffset((2.dp + 18.dp * position).roundToPx(), 0) }.size(24.dp)
                    .dropShadow(
                        CircleShape,
                        Shadow(1.5.dp, Color.Black.copy(alpha = 0.25f), offset = DpOffset(0.dp, 1.dp))
                    )
                    .background(Color.White, CircleShape)
            )
        }
    }
}

@Composable
fun CheckRow(
    title: String,
    checked: Boolean,
    change: (Boolean) -> Unit,
    enabled: Boolean = true,
    leading: (@Composable () -> Unit)? = null
) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val pressed by interaction.collectIsPressedAsState()
    Box(
        Modifier.fillMaxWidth()
            .background(if (pressed || focused) colors.lifted else Color.Transparent, RoundedCornerShape(Radius.sm))
            .toggleable(
                checked,
                interaction,
                indication = null,
                enabled = enabled,
                role = Role.Checkbox,
                onValueChange = change
            )
    ) {
        ListLine(title, leading = leading) {
            val shape = RoundedCornerShape(5.dp)
            Box(
                Modifier.size(20.dp).well(colors, shape)
                    .background(if (checked) colors.accent else Color.Transparent, shape)
                    .border(1.5.dp, if (focused) colors.accentStrong else Color.Transparent, shape),
                contentAlignment = Alignment.Center
            ) {
                if (checked) AppIcon(AppSymbol.CHECK, Modifier.size(12.dp), tint = colors.onAccent)
            }
        }
    }
}

@Composable
fun KodosiCard(
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    radius: Dp = Radius.lg,
    content: @Composable () -> Unit
) {
    val colors = KodosiTheme.colors
    val shape = RoundedCornerShape(radius)
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovered by interaction.collectIsHoveredAsState()
    val focused by interaction.collectIsFocusedAsState()
    val lifted = onClick != null && (pressed || hovered || focused)
    val scale by animateFloatAsState(
        if (pressed) 0.98f else 1f,
        if (reducedMotion()) snap() else spring(dampingRatio = 0.8f, stiffness = 620f),
        label = "card press"
    )
    val rise by animateFloatAsState(
        if (lifted) -1f else 0f,
        if (reducedMotion()) snap() else spring(dampingRatio = 0.8f, stiffness = 620f),
        label = "card lift"
    )
    Box(
        modifier.graphicsLayer {
            scaleX = scale
            scaleY = scale
            translationY = rise * density
        }.raised(
            colors,
            shape,
            fill = if (lifted) colors.lifted else colors.raised,
            elevation = if (lifted) Elevation.LIFTED else Elevation.RESTING
        ).border(1.5.dp, if (focused) colors.accent.copy(alpha = 0.55f) else Color.Transparent, shape)
            .clip(shape).then(
                if (onClick == null) {
                    Modifier
                } else {
                    Modifier.clickable(
                        interaction,
                        indication = null,
                        role = Role.Button,
                        onClick = onClick
                    )
                }
            )
    ) { content() }
}
