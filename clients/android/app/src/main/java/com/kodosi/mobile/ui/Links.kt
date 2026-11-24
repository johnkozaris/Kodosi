package com.kodosi.mobile.ui

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.core.net.toUri
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.NativeRuntime
import com.kodosi.mobile.ui.KodosiTextButton
import org.json.JSONObject

sealed interface LinkReview {
    data class Open(val address: String, val asks: Boolean) : LinkReview

    data class OnComputer(val name: String) : LinkReview

    data object Invalid : LinkReview
}

object LinkPolicy {
    data class InText(val range: IntRange, val address: String)
    private val addresses = Regex("""(?:https?://|mailto:)[^\s<>"'\\]+""", RegexOption.IGNORE_CASE)

    fun inText(text: CharSequence): Sequence<InText> = addresses.findAll(text).map { found ->
        var address = found.value.trimEnd('.', ',', ';')
        for ((open, close) in listOf('(' to ')', '[' to ']', '{' to '}')) {
            while (address.endsWith(close) && address.count { it == close } > address.count { it == open }) {
                address = address.dropLast(1)
            }
        }
        InText(found.range.first until found.range.first + address.length, address)
    }

    /** The runtime decides a link by the rule of every app. A phone opens no file by a link. */
    fun review(raw: String, computer: String? = null): LinkReview {
        val decision = NativeRuntime.linkDecision(raw.encodeToByteArray(), (computer ?: "").encodeToByteArray())
            ?.let { runCatching { JSONObject(it.decodeToString()) }.getOrNull() }
            ?: return LinkReview.Invalid
        val target = decision.optString("target")
        return when (decision.optString("action")) {
            "open" -> if (target.startsWith("file:")) LinkReview.Invalid else LinkReview.Open(target, asks = false)
            "ask" -> LinkReview.Open(target, asks = true)
            else -> when (decision.optString("refusal")) {
                "fileElsewhere", "addressElsewhere" -> LinkReview.OnComputer(decision.optString("place"))
                else -> LinkReview.Invalid
            }
        }
    }
}

fun interface LinkOpener {
    fun open(address: String, computer: String?)

    fun open(address: String) = open(address, null)
}

val LocalLinkOpener = staticCompositionLocalOf<LinkOpener> { LinkOpener { _, _ -> } }

@Composable
fun LinkActions(content: @Composable () -> Unit) {
    val context = LocalContext.current
    var pending by remember { mutableStateOf<Pair<String, LinkReview>?>(null) }
    val opener = remember(context) {
        LinkOpener { address, computer ->
            when (val review = LinkPolicy.review(address, computer)) {
                is LinkReview.Open -> {
                    if (review.asks) {
                        pending = address to review
                    } else if (!launch(context, address)) {
                        pending =
                            address to LinkReview.Invalid
                    }
                }

                else -> pending = address to review
            }
        }
    }
    CompositionLocalProvider(LocalLinkOpener provides opener, content = content)
    pending?.let { (address, review) ->
        KodosiDialog(
            onDismissRequest = { pending = null },
            title = {
                Text(stringResource(if (review is LinkReview.Open) R.string.open_app_link else R.string.open_link))
            },
            text = {
                Text(
                    when (review) {
                        is LinkReview.OnComputer -> stringResource(R.string.link_on_computer, review.name)
                        is LinkReview.Open -> address
                        LinkReview.Invalid -> stringResource(R.string.invalid_link)
                    }
                )
            },
            confirmButton = {
                if (review is LinkReview.Open) {
                    KodosiTextButton(onClick = {
                        launch(context, address)
                        pending = null
                    }) {
                        Text(stringResource(R.string.open_link))
                    }
                } else {
                    KodosiTextButton(onClick = {
                        context.getSystemService(
                            ClipboardManager::class.java
                        ).setPrimaryClip(ClipData.newPlainText("", address))
                        pending = null
                    }) { Text(stringResource(R.string.copy)) }
                }
            },
            dismissButton = { KodosiTextButton(onClick = { pending = null }) { Text(stringResource(R.string.cancel)) } }
        )
    }
}

private fun launch(context: Context, address: String): Boolean = try {
    context.startActivity(Intent(Intent.ACTION_VIEW, address.toUri()).addCategory(Intent.CATEGORY_BROWSABLE))
    true
} catch (_: ActivityNotFoundException) {
    false
} catch (_: SecurityException) {
    false
}
