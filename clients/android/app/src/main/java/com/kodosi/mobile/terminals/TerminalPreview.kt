package com.kodosi.mobile.terminals

import androidx.compose.animation.core.animateFloatAsState
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
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.snappy
import com.kodosi.mobile.ui.terminalTile

// Mobile devices host no terminals, so follow the app's current theme in the settings preview.
@Composable
fun TerminalPreview(adjustment: Int) {
    val colors = KodosiTheme.colors
    val ink = if (colors.isDark) Color(0xFFE8E1D9) else Color(0xFF2A1D15)
    val path = if (colors.isDark) Color(0xFF5DAFA6) else Color(0xFF1F7A74)
    val base = if (LocalConfiguration.current.smallestScreenWidthDp >= 600) 14f else 12f
    val points by animateFloatAsState(
        base + adjustment,
        snappy(),
        label = "terminal preview size"
    )
    val style = TextStyle(
        fontFamily = FontFamily(Font(R.font.jetbrains_mono_regular)),
        fontSize = with(LocalDensity.current) { points.dp.toSp() },
        lineHeight = with(LocalDensity.current) { (points * 1.1f).dp.toSp() },
        color = ink
    )
    val label = stringResource(R.string.terminal_preview)
    Column(
        Modifier.fillMaxWidth().terminalTile(colors, Radius.lg).padding(18.dp).heightIn(min = 92.dp)
            .clearAndSetSemantics { contentDescription = label },
        verticalArrangement = Arrangement.spacedBy((points * 0.1f).dp)
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy((points * 0.5f).dp)) {
            Text("~/kodosi", style = style, color = path)
            Text(">", style = style, color = colors.accent)
            Text("claude", style = style)
        }
        Text(stringResource(R.string.terminal_preview_message), style = style, color = Color(0xFFD97757), maxLines = 1)
        Row(horizontalArrangement = Arrangement.spacedBy((points * 0.5f).dp)) {
            Text("~/kodosi", style = style, color = path)
            Text(">", style = style, color = colors.accent)
            Box(Modifier.size((points * 0.6f).dp, (points * 1.15f).dp).background(colors.accent))
        }
    }
}
