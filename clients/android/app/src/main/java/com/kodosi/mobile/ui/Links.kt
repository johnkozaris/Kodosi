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
import com.kodosi.mobile.ui.KodosiTextButton
import java.net.IDN
import java.net.URI

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

    fun review(raw: String, computer: String? = null): LinkReview {
        if (raw.isBlank() || raw.codePoints().anyMatch {
                Character.isISOControl(it) || Character.getType(it) == Character.FORMAT.toInt() ||
                    Character.isWhitespace(it)
            }
        ) {
            return LinkReview.Invalid
        }
        val uri = runCatching { URI(raw) }.getOrNull() ?: return LinkReview.Invalid
        val scheme = uri.scheme?.lowercase() ?: return LinkReview.Invalid
        if (uri.rawUserInfo != null) return LinkReview.Invalid
        val host = (
            uri.host ?: runCatching {
                val authority = uri.rawAuthority ?: return@runCatching null
                if ('@' in authority || '%' in authority) return@runCatching null
                val name = authority.substringBefore(':')
                val port = authority.substringAfter(':', "")
                if (':' in authority && (port.isEmpty() || port.any { !it.isDigit() })) return@runCatching null
                URI("$scheme://${IDN.toASCII(name, IDN.USE_STD3_ASCII_RULES)}${authority.removePrefix(name)}").host
            }.getOrNull()
            )?.lowercase()?.trim('[', ']')
        if (scheme in setOf("http", "https")) {
            if (host.isNullOrEmpty()) return LinkReview.Invalid
            val local = host == "localhost" || host.endsWith(".localhost") || host == "::1" || host == "::" ||
                host == "0.0.0.0" || host.startsWith("127.") || host.endsWith(".local")
            return if (local && computer != null) LinkReview.OnComputer(computer) else LinkReview.Open(raw, false)
        }
        if (scheme == "file") return if (computer != null) LinkReview.OnComputer(computer) else LinkReview.Invalid
        if (scheme in setOf("javascript", "data", "blob", "content", "intent", "about")) return LinkReview.Invalid
        if (uri.rawSchemeSpecificPart.isNullOrBlank()) return LinkReview.Invalid
        return LinkReview.Open(raw, asks = scheme != "mailto")
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
