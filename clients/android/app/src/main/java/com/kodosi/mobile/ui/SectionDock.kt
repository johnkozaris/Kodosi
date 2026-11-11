package com.kodosi.mobile.ui

import androidx.compose.animation.ExperimentalSharedTransitionApi
import androidx.compose.animation.animateBounds
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.movableContentOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.LookaheadScope
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

data class DockItem(val title: String, val symbol: AppSymbol, val count: Int = 0)

@Composable
fun SectionDock(items: List<DockItem>, selection: Int, select: (Int) -> Unit) {
    val colors = KodosiTheme.colors
    val haptics = LocalHapticFeedback.current
    val motion = if (reducedMotion()) {
        snap()
    } else {
        spring<androidx.compose.ui.unit.IntSize>(
            dampingRatio = 0.8f,
            stiffness = 620f
        )
    }
    Box(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center
    ) {
        PillTrack { thumb ->
            items.forEachIndexed { index, item ->
                val selected = index == selection
                Box(
                    Modifier.defaultMinSize(minWidth = 48.dp, minHeight = 48.dp).animateContentSize(motion)
                        .clip(CircleShape)
                        .selectable(
                            selected,
                            interactionSource = remember {
                                MutableInteractionSource()
                            },
                            indication = null,
                            role = Role.Tab,
                            onClick = {
                                if (!selected) haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                                select(index)
                            }
                        )
                        .semantics {
                            contentDescription = item.title
                            if (item.count > 0) stateDescription = item.count.toString()
                        },
                    contentAlignment = Alignment.Center
                ) {
                    if (selected) thumb(Modifier.matchParentSize())
                    Row(
                        Modifier.defaultMinSize(minHeight = 34.dp).padding(horizontal = 14.dp),
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        AppIcon(
                            item.symbol,
                            Modifier.size(12.dp),
                            tint = if (selected) colors.accentStrong else colors.inkMuted
                        )
                        if (selected) {
                            Text(
                                item.title,
                                style = KodosiType.footnote,
                                color = colors.ink,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.clearAndSetSemantics {}
                            )
                        }
                        if (item.count > 0) {
                            Text(
                                item.count.toString(),
                                style = KodosiType.footnote,
                                color = if (selected) colors.inkMuted else colors.inkFaint,
                                modifier = Modifier.clearAndSetSemantics {}
                            )
                        }
                    }
                }
            }
        }
    }
}

data class SegmentOption(val title: String, val symbol: AppSymbol? = null)

@Composable
fun SegmentedPill(options: List<SegmentOption>, selection: Int, select: (Int) -> Unit) {
    val colors = KodosiTheme.colors
    val haptics = LocalHapticFeedback.current
    PillTrack(faceHeight = 30.dp) { thumb ->
        options.forEachIndexed { index, option ->
            val selected = index == selection
            Box(
                Modifier.defaultMinSize(minWidth = 48.dp, minHeight = 48.dp).selectable(
                    selected,
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    role = Role.Tab,
                    onClick = {
                        if (!selected) haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                        select(index)
                    }
                ),
                contentAlignment = Alignment.Center
            ) {
                if (selected) thumb(Modifier.matchParentSize())
                Row(
                    Modifier.defaultMinSize(minHeight = 30.dp).padding(horizontal = 12.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    option.symbol?.let {
                        AppIcon(it, Modifier.size(11.dp), tint = if (selected) colors.ink else colors.inkMuted)
                    }
                    Text(
                        option.title,
                        style = KodosiType.footnote,
                        color = if (selected) colors.ink else colors.inkMuted,
                        maxLines = 1
                    )
                }
            }
        }
    }
}

@OptIn(ExperimentalSharedTransitionApi::class)
@Composable
private fun PillTrack(faceHeight: Dp = 34.dp, content: @Composable RowScope.(@Composable (Modifier) -> Unit) -> Unit) {
    val colors = KodosiTheme.colors
    val reduced = reducedMotion()
    LookaheadScope {
        val scope = this
        val thumb = remember(colors) {
            movableContentOf<Modifier> { modifier -> Box(modifier.raised(colors, CircleShape)) }
        }
        Box {
            Box(
                Modifier.matchParentSize().padding(vertical = (48.dp - faceHeight - 6.dp) / 2)
                    .well(colors, CircleShape)
            )
            Row(
                Modifier.padding(horizontal = 3.dp).selectableGroup(),
                horizontalArrangement = Arrangement.spacedBy(2.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                content { modifier ->
                    val face = modifier.padding(vertical = (48.dp - faceHeight) / 2)
                    thumb(
                        if (reduced) {
                            face
                        } else {
                            face.animateBounds(
                                scope,
                                boundsTransform = { _, _ -> spring(dampingRatio = 0.8f, stiffness = 620f) }
                            )
                        }
                    )
                }
            }
        }
    }
}
