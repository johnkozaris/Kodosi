package com.kodosi.mobile.ui

import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R

@Composable
fun MessageComposer(
    text: String,
    busy: Boolean,
    change: (String) -> Unit,
    send: () -> Unit,
    enabled: Boolean = true,
    focusRequester: FocusRequester? = null
) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val shape = RoundedCornerShape(20.dp)
    val label = stringResource(R.string.write_message)
    Row(
        Modifier.fillMaxWidth().padding(10.dp)
            .raised(
                colors,
                shape,
                fill = colors.lifted,
                elevation = if (focused) Elevation.LIFTED else Elevation.RESTING
            )
            .border(1.5.dp, if (focused) lerp(colors.accent, colors.lifted, 0.5f) else Color.Transparent, shape)
            .padding(5.dp),
        verticalAlignment = Alignment.Bottom,
        horizontalArrangement = Arrangement.spacedBy(4.dp)
    ) {
        BasicTextField(
            value = text,
            onValueChange = change,
            maxLines = 6,
            interactionSource = interaction,
            textStyle = KodosiType.body.copy(color = if (enabled) colors.ink else colors.inkFaint),
            cursorBrush = SolidColor(colors.accent),
            modifier = Modifier.weight(1f)
                .then(if (focusRequester != null) Modifier.focusRequester(focusRequester) else Modifier)
                .padding(horizontal = 12.dp, vertical = 11.dp).semantics { contentDescription = label },
            enabled = enabled,
            decorationBox = { input ->
                Box {
                    if (text.isEmpty()) Text(label, style = KodosiType.body, color = colors.inkMuted)
                    input()
                }
            }
        )
        KodosiSendKey(enabled && !busy && text.isNotBlank(), busy, send)
    }
}
