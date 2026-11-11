package com.kodosi.mobile.terminals

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import com.kodosi.mobile.ui.DarkColors
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.reducedMotion
import com.kodosi.mobile.ui.terminalTile

@Composable
fun TerminalPreview(adjustment: Int) {
    val base = if (LocalConfiguration.current.smallestScreenWidthDp >= 600) 14f else 12f
    val points by animateFloatAsState(
        base + adjustment,
        if (reducedMotion()) snap() else spring(dampingRatio = 0.8f, stiffness = 620f),
        label = "terminal preview size"
    )
    val style = TextStyle(
        fontFamily = FontFamily(Font(R.font.jetbrains_mono_regular)),
        fontSize = with(LocalDensity.current) { points.dp.toSp() },
        lineHeight = with(LocalDensity.current) { (points * 1.1f).dp.toSp() },
        color = Color(0xFFE8E1D9)
    )
    val label = stringResource(R.string.terminal_preview)
    Column(
        Modifier.fillMaxWidth().terminalTile(DarkColors, Radius.lg).padding(18.dp).heightIn(min = 92.dp)
            .clearAndSetSemantics { contentDescription = label },
        verticalArrangement = Arrangement.spacedBy((points * 0.1f).dp)
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy((points * 0.5f).dp)) {
            Text("~/kodosi", style = style, color = Color(0xFF5DAFA6))
            Text(">", style = style, color = DarkColors.accent)
            Text("claude", style = style)
        }
        Text(stringResource(R.string.terminal_preview_message), style = style, color = Color(0xFFD97757), maxLines = 1)
        Row(horizontalArrangement = Arrangement.spacedBy((points * 0.5f).dp)) {
            Text("~/kodosi", style = style, color = Color(0xFF5DAFA6))
            Text(">", style = style, color = DarkColors.accent)
            Box(Modifier.size((points * 0.6f).dp, (points * 1.15f).dp).background(DarkColors.accent))
        }
    }
}
