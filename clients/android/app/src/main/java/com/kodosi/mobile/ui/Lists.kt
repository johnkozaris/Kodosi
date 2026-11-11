package com.kodosi.mobile.ui

import androidx.compose.foundation.BorderStroke
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
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.constrainHeight
import androidx.compose.ui.unit.dp

object TileTint {
    val orange = Color(0xFFD98A5F)
    val amber = Color(0xFFD9A441)
    val green = Color(0xFF5FA57E)
    val teal = Color(0xFF4F9BB0)
    val blue = Color(0xFF5B82C9)
    val purple = Color(0xFF8C7BD1)
    val pink = Color(0xFFC96F8E)
    val red = Color(0xFFCF6A62)
    val graphite = Color(0xFF6B6560)
}

@Composable
fun IconTile(symbol: AppSymbol, tint: Color, size: Dp = 28.dp) {
    val colors = KodosiTheme.colors
    val shape = RoundedCornerShape(size * 0.3f)
    Box(
        Modifier.size(size)
            .dropShadow(
                shape,
                Shadow(
                    size * 0.08f,
                    colors.shadow.copy(alpha = if (colors.isDark) 0.4f else 0.18f),
                    offset = DpOffset(1.dp, 2.dp)
                )
            )
            .background(
                Brush.linearGradient(listOf(lerp(tint, Color.White, 0.22f), tint, lerp(tint, Color.Black, 0.2f))),
                shape
            )
            .border(
                BorderStroke(
                    0.75.dp,
                    Brush.linearGradient(
                        listOf(Color.White.copy(alpha = 0.4f), Color.Transparent, Color.Black.copy(alpha = 0.18f))
                    )
                ),
                shape
            ),
        contentAlignment = Alignment.Center
    ) {
        AppIcon(symbol, Modifier.size(size * 0.46f), tint = Color.White)
    }
}

@Composable
fun ListButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    content: @Composable () -> Unit
) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovered by interaction.collectIsHoveredAsState()
    val focused by interaction.collectIsFocusedAsState()
    Box(
        modifier.fillMaxWidth()
            .background(if (enabled && (pressed || hovered || focused)) colors.lifted else Color.Transparent)
            .clickable(interaction, indication = null, enabled = enabled, role = Role.Button, onClick = onClick)
    ) { content() }
}

@Composable
fun SwitchRow(
    title: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    symbol: AppSymbol,
    tint: Color,
    subtitle: String? = null,
    enabled: Boolean = true
) {
    ListLine(title, subtitle = subtitle, leading = { IconTile(symbol, tint) }) {
        KodosiSwitch(checked, onCheckedChange, title, enabled)
    }
}

@Composable
fun MenuPicker(
    title: String,
    value: String,
    options: List<Pair<String, String>>,
    leading: (@Composable () -> Unit)? = null,
    enabled: Boolean = true,
    details: Map<String, String> = emptyMap(),
    choose: (String) -> Unit
) {
    val colors = KodosiTheme.colors
    var expanded by remember { mutableStateOf(false) }
    Box {
        val row: @Composable () -> Unit = {
            ListLine(title, leading = leading) {
                androidx.compose.foundation.layout.Row(
                    horizontalArrangement = Arrangement.spacedBy(7.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        value,
                        modifier = Modifier.weight(1f, fill = false),
                        style = KodosiType.body,
                        color = colors.inkMuted,
                        maxLines = 1,
                        overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis
                    )
                    if (options.size > 1) AppIcon(AppSymbol.CHEVRONS, Modifier.size(10.dp), tint = colors.inkFaint)
                }
            }
        }
        if (options.size > 1) {
            ListButton(onClick = { expanded = true }, enabled = enabled, content = row)
        } else {
            row()
        }
        KodosiMenu(expanded && enabled, { expanded = false }) {
            options.forEach { (id, name) ->
                KodosiMenuItem(text = {
                    Column(Modifier.widthIn(max = 320.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(name)
                        details[id]?.let {
                            Text(it, style = KodosiType.footnote, color = colors.inkMuted)
                        }
                    }
                }, onClick = {
                    expanded = false
                    choose(id)
                })
            }
        }
    }
}

@Composable
fun ListGroup(
    modifier: Modifier = Modifier,
    title: String? = null,
    footer: String? = null,
    content: @Composable () -> Unit
) {
    val colors = KodosiTheme.colors
    val shape = RoundedCornerShape(Radius.xl)
    val separators = remember { mutableStateListOf<Int>() }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        title?.let {
            Text(
                it,
                style = KodosiType.subhead,
                color = colors.inkMuted,
                modifier = Modifier.padding(start = 4.dp).semantics { heading() }
            )
        }

        Layout(
            content = content,
            modifier = Modifier.fillMaxWidth().raised(colors, shape).clip(shape).drawWithContent {
                drawContent()
                separators.forEach { y ->
                    drawLine(
                        colors.hairline.copy(alpha = 0.55f),
                        Offset(54.dp.toPx(), y.toFloat()),
                        Offset(size.width, y.toFloat()),
                        0.5.dp.toPx()
                    )
                }
            }

        ) { rows, constraints ->
            val rowConstraints = constraints.copy(minHeight = 0, maxHeight = Constraints.Infinity)
            val measured = rows.map { it.measure(rowConstraints) }
            separators.clear()
            var height = 0
            measured.forEachIndexed { index, row ->
                if (index > 0) separators.add(height)
                height += row.height
            }
            layout(constraints.maxWidth, constraints.constrainHeight(height)) {
                var y = 0
                measured.forEach { row ->
                    row.placeRelative(0, y)
                    y += row.height
                }
            }
        }
        footer?.let {
            Text(
                it,
                style = KodosiType.caption,
                color = colors.inkFaint,
                modifier = Modifier.padding(horizontal = 4.dp)
            )
        }
    }
}
