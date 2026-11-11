package com.kodosi.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

data class NavigationItem(val title: String, val symbol: AppSymbol, val count: Int = 0)

@Composable
fun KodosiNavigation(items: List<NavigationItem>, selection: Int, rail: Boolean, select: (Int) -> Unit) {
    if (rail) {
        Column(
            Modifier.widthIn(min = 88.dp, max = 160.dp).width(IntrinsicSize.Max)
                .fillMaxHeight().statusBarsPadding().navigationBarsPadding().padding(top = 20.dp)
                .selectableGroup(),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            items.forEachIndexed { index, item ->
                NavigationKey(item, index == selection, Modifier.fillMaxWidth().padding(horizontal = 6.dp)) {
                    select(index)
                }
            }
        }
    } else {
        Row(
            Modifier.fillMaxWidth().navigationBarsPadding().selectableGroup().padding(
                horizontal = 8.dp,
                vertical = 4.dp
            ),
            verticalAlignment = Alignment.CenterVertically
        ) {
            items.forEachIndexed { index, item ->
                NavigationKey(item, index == selection, Modifier.weight(1f)) { select(index) }
            }
        }
    }
}

@Composable
private fun NavigationKey(item: NavigationItem, selected: Boolean, modifier: Modifier, onClick: () -> Unit) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovered by interaction.collectIsHoveredAsState()
    val focused by interaction.collectIsFocusedAsState()
    Column(
        modifier.defaultMinSize(minHeight = 56.dp)
            .background(
                if (pressed || hovered || focused) colors.ink.copy(alpha = 0.07f) else Color.Transparent,
                RoundedCornerShape(Radius.md)
            )
            .selectable(
                selected,
                interaction,
                indication = null,
                role = Role.Tab,
                onClick = onClick
            ).semantics { if (item.count > 0) stateDescription = item.count.toString() }
            .padding(horizontal = 4.dp, vertical = 6.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(3.dp, Alignment.CenterVertically)
    ) {
        Box(Modifier.size(22.dp)) {
            AppIcon(
                item.symbol,
                Modifier.matchParentSize(),
                tint = if (selected) colors.accentStrong else colors.inkMuted
            )
            if (item.count > 0) {
                Box(
                    Modifier.matchParentSize().wrapContentSize(Alignment.TopStart, unbounded = true)
                        .offset(x = 16.dp, y = (-5).dp).clearAndSetSemantics {}
                ) { CountBadge(item.count) }
            }
        }
        Text(
            item.title,
            style = KodosiType.caption,
            color = if (selected) colors.accentStrong else colors.inkMuted,
            textAlign = TextAlign.Center,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Ellipsis
        )
    }
}

@Composable
fun CountBadge(count: Int, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    Text(
        if (count > 99) "99+" else count.toString(),
        modifier.background(colors.accent, RoundedCornerShape(50)).defaultMinSize(17.dp, 17.dp)
            .padding(horizontal = 5.dp),
        style = KodosiType.caption,
        color = colors.onAccent,
        textAlign = TextAlign.Center
    )
}
