package com.kodosi.mobile.chat

import android.content.Context
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.net.Uri
import android.util.Size
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.StringRes
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.RuntimeFailure
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ComposerPictures
import com.kodosi.mobile.ui.KodosiQuietKey
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.Unfolding
import com.kodosi.mobile.ui.bezel
import com.kodosi.mobile.ui.snappy
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.nio.ByteBuffer
import kotlin.math.roundToInt
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** A picture of the next message: its bytes as the agent gets them, and a small view of it. */
class DraftPicture(val bytes: ByteArray, val thumbnail: Painter)

internal object ChatPictures {
    /** The pictures of one message as the terminal's computer takes them: at most [MOST], of [MOST_BYTES] together. */
    const val MOST = 8
    const val MOST_BYTES = 32 * 1024 * 1024

    /** The longest side of a picture that Kodosi turns into a PNG or a JPEG. */
    private const val MOST_SIDE = 4096
    private const val JPEG_QUALITY = 90

    private val PNG = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
    private val JPEG = byteArrayOf(0xFF.toByte(), 0xD8.toByte(), 0xFF.toByte())
    private val GIF87 = "GIF87a".encodeToByteArray()
    private val GIF89 = "GIF89a".encodeToByteArray()
    private val RIFF = "RIFF".encodeToByteArray()
    private val WEBP = "WEBP".encodeToByteArray()

    /**
     * The picture at [uri] as the agents take it: a PNG, JPEG, GIF or WebP as it is, and a picture of
     * another kind, such as HEIC, as a PNG or a JPEG. Its thumbnail has [side] pixels on its shorter side.
     */
    suspend fun read(context: Context, uri: Uri, side: Int): DraftPicture = withContext(Dispatchers.IO) {
        fun refused(@StringRes words: Int): Nothing = throw RuntimeFailure(context.getString(words))
        try {
            val given = context.contentResolver.openInputStream(uri)?.use { it.readNBytes(MOST_BYTES + 1) }
                ?: refused(R.string.chat_picture_unreadable)
            if (given.size > MOST_BYTES) refused(R.string.chat_pictures_too_many)
            val bytes = if (kept(given)) given else converted(given)
            if (bytes.size > MOST_BYTES) refused(R.string.chat_pictures_too_many)
            DraftPicture(bytes, BitmapPainter(thumbnail(bytes, side).asImageBitmap()))
        } catch (_: IOException) {
            refused(R.string.chat_picture_unreadable)
        } catch (_: SecurityException) {
            refused(R.string.chat_picture_unreadable)
        }
    }

    /** A PNG, JPEG, GIF or WebP, by the signature that the terminal's computer reads. */
    private fun kept(bytes: ByteArray): Boolean = bytes.holds(PNG) || bytes.holds(JPEG) || bytes.holds(GIF87) ||
        bytes.holds(GIF89) || (bytes.holds(RIFF) && bytes.holds(WEBP, at = 8))

    private fun ByteArray.holds(signature: ByteArray, at: Int = 0): Boolean =
        size >= at + signature.size && signature.indices.all { this[at + it] == signature[it] }

    /** A PNG when the picture has transparent parts, else a JPEG. */
    private fun converted(bytes: ByteArray): ByteArray {
        val bitmap = ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(bytes))) { decoder, info, _ ->
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            decoder.scale(info.size, MOST_SIDE.toFloat() / maxOf(info.size.width, info.size.height))
        }
        val written = ByteArrayOutputStream()
        if (bitmap.hasAlpha()) {
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, written)
        } else {
            bitmap.compress(Bitmap.CompressFormat.JPEG, JPEG_QUALITY, written)
        }
        bitmap.recycle()
        return written.toByteArray()
    }

    private fun thumbnail(bytes: ByteArray, side: Int): Bitmap =
        ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(bytes))) { decoder, info, _ ->
            decoder.scale(info.size, side.toFloat() / minOf(info.size.width, info.size.height))
        }

    /** Decodes the picture at [scale] times its [size] when that makes it smaller. */
    private fun ImageDecoder.scale(size: Size, scale: Float) {
        if (scale >= 1f) return
        setTargetSize(
            (size.width * scale).roundToInt().coerceAtLeast(1),
            (size.height * scale).roundToInt().coerceAtLeast(1)
        )
    }
}

/**
 * The pictures of the chat's next message in the dock, and why the last pictures that the person gave
 * did not all join the message.
 */
internal class DockPictures(
    val attached: List<DraftPicture>,
    refused: MutableState<String?>,
    private val take: (List<Uri>) -> Unit,
    private val pick: () -> Unit,
    private val remove: (DraftPicture) -> Unit
) {
    var refusal: String? by refused

    fun composer(enabled: Boolean, sendable: Boolean) = ComposerPictures(
        sendable,
        take,
        key = {
            KodosiQuietKey(
                AppSymbol.PICTURE,
                stringResource(R.string.chat_add_picture),
                pick,
                Modifier.testTag("chat.picture.add"),
                enabled
            )
        },
        attached = { Unfolding(attached.isNotEmpty()) { DraftPictureRow(attached, remove) } }
    )
}

/** The pictures of the dock, which the picture key takes from the system's photo picker. */
@Composable
internal fun rememberDockPictures(chat: TerminalChat): DockPictures {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val side = with(LocalDensity.current) { DraftThumbnailSide.roundToPx() }
    val tooMany = stringResource(R.string.chat_pictures_too_many)
    val attached by chat.pictures.collectAsStateWithLifecycle()
    val refused = remember(chat) { mutableStateOf<String?>(null) }
    val take: (List<Uri>) -> Unit = { given ->
        refused.value = null
        scope.launch {
            for (uri in given) {
                try {
                    if (!chat.attach(ChatPictures.read(context, uri, side))) refused.value = tooMany
                } catch (failure: RuntimeFailure) {
                    refused.value = failure.message
                }
            }
        }
    }
    val picker = rememberLauncherForActivityResult(
        remember { ActivityResultContracts.PickMultipleVisualMedia(ChatPictures.MOST) },
        take
    )
    val pick = { picker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }
    return DockPictures(attached, refused, take, pick) { picture ->
        refused.value = null
        chat.detach(picture)
    }
}

internal val DraftThumbnailSide = 56.dp
private val RemoveReach = 10.dp

/** The pictures of the next message above the composer's words, each with its remove key. */
@Composable
internal fun DraftPictureRow(pictures: List<DraftPicture>, remove: (DraftPicture) -> Unit) {
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())
            .padding(start = 7.dp, end = 7.dp + RemoveReach, top = RemoveReach, bottom = 2.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        pictures.forEachIndexed { index, picture ->
            key(picture) { DraftThumbnail(picture, index) { remove(picture) } }
        }
    }
}

@Composable
private fun DraftThumbnail(picture: DraftPicture, index: Int, remove: () -> Unit) {
    val colors = KodosiTheme.colors
    val shape = RoundedCornerShape(Radius.md)
    val removeWords = stringResource(R.string.remove)
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(if (pressed) 0.9f else 1f, snappy(), label = "remove press")
    Box(Modifier.testTag("chat.draft.picture.$index")) {
        Image(
            picture.thumbnail,
            stringResource(R.string.chat_picture),
            Modifier.size(DraftThumbnailSide).clip(shape).border(0.5.dp, colors.hairline, shape),
            contentScale = ContentScale.Crop
        )
        // The key's touch reaches past the corner of the picture to take 48 dp.
        Box(
            Modifier.align(Alignment.TopEnd).offset(x = RemoveReach, y = -RemoveReach).size(48.dp)
                .testTag("chat.draft.picture.remove.$index")
                .clickable(interaction, indication = null, role = Role.Button, onClick = remove)
                .semantics { contentDescription = removeWords },
            contentAlignment = Alignment.Center
        ) {
            Box(
                Modifier.size(22.dp).graphicsLayer {
                    scaleX = scale
                    scaleY = scale
                }.bezel(colors, CircleShape, colors.raised),
                contentAlignment = Alignment.Center
            ) {
                AppIcon(AppSymbol.CLOSE, Modifier.size(10.dp), tint = colors.ink)
            }
        }
    }
}

private val PictureTile = 24.dp

/** A small tile for each picture of a message, in a bubble of [fill]. */
@Composable
internal fun PictureTiles(pictures: List<ChatPicture>, fill: Color) {
    val colors = KodosiTheme.colors
    val words = stringResource(R.string.chat_picture)
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        repeat(pictures.size) {
            Box(
                Modifier.size(PictureTile).background(lerp(fill, colors.ink, 0.08f), RoundedCornerShape(Radius.xs))
                    .clearAndSetSemantics { contentDescription = words },
                contentAlignment = Alignment.Center
            ) {
                AppIcon(AppSymbol.PICTURE, Modifier.size(13.dp), tint = colors.inkMuted)
            }
        }
    }
}
