package com.kodosi.mobile.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R

@Composable
fun MessageComposer(
    draft: TextFieldState,
    busy: Boolean,
    send: () -> Unit,
    enabled: Boolean = true,
    focusRequester: FocusRequester? = null,
    placeholder: String = stringResource(R.string.write_message),
    stop: (() -> Unit)? = null,
    talk: (() -> Unit)? = null
) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val shape = RoundedCornerShape(20.dp)
    Row(
        Modifier.fillMaxWidth().padding(10.dp)
            .raised(
                colors,
                shape,
                fill = colors.lifted,
                elevation = if (focused) Elevation.LIFTED else Elevation.RESTING
            )
            .padding(5.dp),
        verticalAlignment = Alignment.Bottom,
        horizontalArrangement = Arrangement.spacedBy(4.dp)
    ) {
        BasicTextField(
            draft,
            lineLimits = TextFieldLineLimits.MultiLine(maxHeightInLines = 6),
            interactionSource = interaction,
            textStyle = KodosiType.body.copy(color = if (enabled) colors.ink else colors.inkFaint),
            cursorBrush = SolidColor(colors.accent),
            modifier = Modifier.weight(1f)
                .then(if (focusRequester != null) Modifier.focusRequester(focusRequester) else Modifier)
                .padding(horizontal = 12.dp, vertical = 11.dp).semantics { contentDescription = placeholder },
            enabled = enabled,
            decorator = { input ->
                Box {
                    if (draft.text.isEmpty()) Text(placeholder, style = KodosiType.body, color = colors.inkMuted)
                    input()
                }
            }
        )
        val fading = fade<Float>()
        val popping = snappy<Float>()
        val blank = draft.text.isBlank()
        val talks = talk != null && blank
        Row {
            AnimatedVisibility(
                talks,
                enter = fadeIn(fading) + scaleIn(popping, initialScale = 0.9f) + expandHorizontally(fade()),
                exit = fadeOut(fading) + scaleOut(popping, targetScale = 0.9f) + shrinkHorizontally(fade()),
                label = "talk key"
            ) { KodosiQuietKey(AppSymbol.MIC, stringResource(R.string.talk), { talk?.invoke() }, enabled) }
            AnimatedContent(
                when {
                    stop != null && blank -> TrailingKey.STOP
                    talks -> TrailingKey.NONE
                    else -> TrailingKey.SEND
                },
                transitionSpec = {
                    (fadeIn(fading) + scaleIn(popping, initialScale = 0.9f)) togetherWith
                        (fadeOut(fading) + scaleOut(popping, targetScale = 0.9f))
                },
                label = "composer key"
            ) { key ->
                when {
                    key == TrailingKey.NONE -> Unit
                    key == TrailingKey.STOP && stop != null -> KodosiStopKey(stop)
                    else -> KodosiSendKey(enabled && !busy && draft.text.isNotBlank(), busy, send)
                }
            }
        }
    }
}

/** The key after the talk key: the stop key while the agent works, the send key, or none. */
private enum class TrailingKey { SEND, STOP, NONE }
