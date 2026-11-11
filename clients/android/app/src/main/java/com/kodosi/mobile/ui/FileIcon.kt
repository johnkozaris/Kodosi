package com.kodosi.mobile.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.GenericShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp

@Composable
fun FileIcon(name: String, kind: String, size: Dp = 36.dp, missing: Boolean = false) {
    val colors = KodosiTheme.colors
    val look = remember(name, kind) { fileLook(name, kind) }
    val tint = if (missing) colors.inkFaint else look.color
    Box(Modifier.size(size).clearAndSetSemantics {}, contentAlignment = Alignment.Center) {
        Box(
            Modifier.size(size * 0.78f, size)
                .dropShadow(
                    fileSheet,
                    Shadow(
                        size * 0.06f,
                        colors.shadow.copy(alpha = if (colors.isDark) 0.4f else 0.18f),
                        offset = DpOffset(0.5.dp, 1.dp)
                    )
                )
                .background(
                    Brush.linearGradient(listOf(lerp(tint, Color.White, 0.22f), tint, lerp(tint, Color.Black, 0.2f))),
                    fileSheet
                )
                .border(
                    BorderStroke(
                        0.75.dp,
                        Brush.linearGradient(
                            listOf(Color.White.copy(alpha = 0.4f), Color.Transparent, Color.Black.copy(alpha = 0.18f))
                        )
                    ),
                    fileSheet
                ),
            contentAlignment = Alignment.Center
        ) {
            Box(Modifier.padding(top = size * 0.14f), contentAlignment = Alignment.Center) {
                if (look.letters != null) {
                    Text(
                        look.letters,
                        color = Color.White,
                        fontWeight = FontWeight.Black,
                        fontSize = with(LocalDensity.current) {
                            (size * if (look.letters.length > 1) 0.27f else 0.42f).toSp()
                        },
                        maxLines = 1
                    )
                } else {
                    AppIcon(look.symbol, Modifier.size(size * 0.34f), tint = Color.White)
                }
            }
            Canvas(Modifier.align(Alignment.TopEnd).size(size * 0.28f)) {
                val radius = this.size.height * (0.05f / 0.28f)
                val fold = Path().apply {
                    moveTo(0f, 0f)
                    lineTo(this@Canvas.size.width, this@Canvas.size.height)
                    lineTo(radius, this@Canvas.size.height)
                    arcTo(
                        Rect(0f, this@Canvas.size.height - 2 * radius, 2 * radius, this@Canvas.size.height),
                        90f,
                        90f,
                        false
                    )
                    close()
                }
                drawPath(fold, lerp(tint, Color.White, 0.45f))
            }
        }
    }
}

private val fileSheet = GenericShape { size, _ ->
    val width = size.width
    val height = size.height
    val radius = height * 0.12f
    val fold = height * 0.28f
    moveTo(radius, 0f)
    lineTo(width - fold, 0f)
    lineTo(width, fold)
    lineTo(width, height - radius)
    arcTo(Rect(width - 2 * radius, height - 2 * radius, width, height), 0f, 90f, false)
    lineTo(radius, height)
    arcTo(Rect(0f, height - 2 * radius, 2 * radius, height), 90f, 90f, false)
    lineTo(0f, radius)
    arcTo(Rect(0f, 0f, 2 * radius, 2 * radius), 180f, 90f, false)
    close()
}

private data class FileLook(val color: Color, val letters: String? = null, val symbol: AppSymbol = AppSymbol.TEXT)

private fun fileLook(name: String, kind: String): FileLook = when (name.substringAfterLast('.', "").lowercase()) {
    "doc", "docx", "docm", "dotx" -> FileLook(Color(0xFF2F64C0), "W")

    "xls", "xlsx", "xlsm", "xltx" -> FileLook(Color(0xFF1F8A52), "X")

    "ppt", "pptx", "pptm", "potx" -> FileLook(Color(0xFFD2552F), "P")

    "pdf" -> FileLook(Color(0xFFD8463E), "PDF")

    "md", "markdown" -> FileLook(Color(0xFF5E6B85), symbol = AppSymbol.HASH)

    "json" -> FileLook(Color(0xFFD9A441), symbol = AppSymbol.BRACES)

    "yaml", "yml" -> FileLook(Color(0xFFC2566E), symbol = AppSymbol.LIST)

    "toml" -> FileLook(Color(0xFF8C7BD1), symbol = AppSymbol.SETTINGS)

    "html", "htm" -> FileLook(Color(0xFFE06A3B), symbol = AppSymbol.CODE)

    "xml" -> FileLook(Color(0xFF4F9BB0), symbol = AppSymbol.CODE)

    "csv", "tsv" -> FileLook(Color(0xFF3F9E6B), symbol = AppSymbol.SHEET)

    "diff", "patch" -> FileLook(Color(0xFF5B82C9), symbol = AppSymbol.DIFF)

    "txt", "log" -> FileLook(Color(0xFF7A6F68))

    else -> when (kind) {
        "picture" -> FileLook(Color(0xFF4F9BB0), symbol = AppSymbol.PICTURE)
        "pdf" -> FileLook(Color(0xFFD8463E), "PDF")
        "document" -> FileLook(Color(0xFF2F64C0))
        "sheet" -> FileLook(Color(0xFF3F9E6B), symbol = AppSymbol.SHEET)
        "presentation" -> FileLook(Color(0xFFD2552F), symbol = AppSymbol.PRESENTATION)
        else -> FileLook(Color(0xFF7A6F68))
    }
}
