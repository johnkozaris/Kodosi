package com.kodosi.mobile.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import kotlin.math.floor

/** The card stays light in both appearances, as a printed code does, so that each camera reads its
 *  dark modules. */
@Composable
fun ScanCodeCard(modifier: Modifier = Modifier, content: @Composable BoxScope.() -> Unit) {
    KodosiTheme(dark = false) {
        Box(
            modifier.aspectRatio(1f).raised(KodosiTheme.colors, RoundedCornerShape(Radius.xl)),
            contentAlignment = Alignment.Center,
            content = content
        )
    }
}

@Composable
fun ScanCode(rows: List<String>, description: String) {
    val ink = KodosiTheme.colors.ink
    Canvas(
        Modifier.fillMaxSize().semantics {
            contentDescription = description
            role = Role.Image
        }
    ) {
        val module = floor(size.minDimension / (rows.size + 2 * QUIET_ZONE))
        val left = floor((size.width - module * rows.size) / 2)
        val top = floor((size.height - module * rows.size) / 2)
        val dark = Path()
        rows.forEachIndexed { y, row ->
            row.forEachIndexed { x, cell ->
                if (cell == '1') {
                    dark.addRect(Rect(Offset(left + x * module, top + y * module), Size(module, module)))
                }
            }
        }
        drawPath(dark, ink)
    }
}

private const val QUIET_ZONE = 4
