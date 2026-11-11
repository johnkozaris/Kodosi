package com.kodosi.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.ProvideTextStyle
import androidx.compose.material3.SnackbarData
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.PopupProperties
import com.kodosi.mobile.R

@Composable
fun KodosiNotice(data: SnackbarData) {
    val colors = KodosiTheme.colors
    Row(
        Modifier.padding(16.dp).widthIn(max = 560.dp).fillMaxWidth()
            .raised(colors, RoundedCornerShape(Radius.lg), elevation = Elevation.FLOATING)
            .semantics { liveRegion = LiveRegionMode.Polite }
            .padding(start = 16.dp, end = 8.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        Text(data.visuals.message, Modifier.weight(1f), style = KodosiType.callout, color = colors.ink)
        data.visuals.actionLabel?.let { label ->
            KodosiTextButton(onClick = data::performAction) { Text(label) }
        }
        ActionIcon(AppSymbol.CLOSE, R.string.close, data::dismiss)
    }
}

@Composable
fun KodosiDialog(
    onDismissRequest: () -> Unit,
    title: @Composable () -> Unit,
    confirmButton: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    text: (@Composable () -> Unit)? = null,
    dismissButton: (@Composable () -> Unit)? = null,
    properties: DialogProperties = DialogProperties()
) {
    val colors = KodosiTheme.colors
    Dialog(onDismissRequest, properties = properties) {
        Column(
            modifier.widthIn(min = 280.dp, max = 560.dp).fillMaxWidth()
                .raised(colors, RoundedCornerShape(Radius.sheet), elevation = Elevation.FLOATING)
                .padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            CompositionLocalProvider(LocalContentColor provides colors.ink) {
                Box(Modifier.semantics(mergeDescendants = true) { heading() }) {
                    ProvideTextStyle(KodosiType.title, title)
                }
            }
            text?.let {
                Box(Modifier.weight(1f, fill = false).fillMaxWidth()) {
                    CompositionLocalProvider(LocalContentColor provides colors.inkMuted) {
                        ProvideTextStyle(KodosiType.body, it)
                    }
                }
            }
            FlowRow(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                verticalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                dismissButton?.invoke()
                confirmButton()
            }
        }
    }
}

@Composable
fun KodosiMenu(
    expanded: Boolean,
    onDismissRequest: () -> Unit,
    modifier: Modifier = Modifier,
    properties: PopupProperties = PopupProperties(focusable = true),
    content: @Composable ColumnScope.() -> Unit
) {
    val colors = KodosiTheme.colors
    val shape = RoundedCornerShape(12.dp)
    DropdownMenu(
        expanded,
        onDismissRequest,
        modifier = modifier.raised(colors, shape, elevation = Elevation.FLOATING),
        properties = properties,
        shape = shape,
        containerColor = colors.raised,
        tonalElevation = 0.dp,
        shadowElevation = 0.dp,
        content = content
    )
}

@Composable
fun KodosiMenuItem(
    text: @Composable () -> Unit,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true
) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val hovered by interaction.collectIsHoveredAsState()
    val pressed by interaction.collectIsPressedAsState()
    val focused by interaction.collectIsFocusedAsState()
    Row(
        modifier.fillMaxWidth().padding(horizontal = 6.dp)
            .background(
                if (enabled && (hovered || pressed || focused)) colors.ink.copy(alpha = 0.08f) else Color.Transparent,
                RoundedCornerShape(Radius.sm)
            )
            .clickable(interaction, indication = null, enabled = enabled, role = Role.Button, onClick = onClick)
            .defaultMinSize(minHeight = 48.dp)
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        CompositionLocalProvider(LocalContentColor provides if (enabled) colors.ink else colors.inkFaint) {
            ProvideTextStyle(KodosiType.body, text)
        }
    }
}
