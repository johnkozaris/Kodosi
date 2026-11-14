package com.kodosi.mobile.ui

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import android.util.Size
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.compose.CameraXViewfinder
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.core.SurfaceRequest
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.lifecycle.awaitInstance
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.google.zxing.BinaryBitmap
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.ReaderException
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import com.kodosi.mobile.R
import java.util.concurrent.Executors
import kotlinx.coroutines.awaitCancellation

fun canScanCodes(context: Context): Boolean = context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)

/** What a scanner says for the kind of code that it reads. */
data class CodeScanWords(val title: String, val hint: String, val other: String, val unavailable: String)

@Composable
fun CodeScanDialog(words: CodeScanWords, accepts: (String) -> String?, found: (String) -> Unit, dismiss: () -> Unit) {
    val context = LocalContext.current
    val colors = KodosiTheme.colors
    var camera by remember {
        mutableStateOf(
            true.takeIf {
                ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                    PackageManager.PERMISSION_GRANTED
            }
        )
    }
    var failed by remember { mutableStateOf(false) }
    var otherCode by remember { mutableStateOf(false) }
    var done by remember { mutableStateOf(false) }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { camera = it }
    LaunchedEffect(Unit) {
        if (camera == null) {
            ask.launch(Manifest.permission.CAMERA)
        }
    }
    Dialog(dismiss, DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        Column(
            Modifier.fillMaxSize().background(colors.surface).systemBarsPadding().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    words.title,
                    style = KodosiType.headline,
                    color = colors.ink,
                    modifier = Modifier.weight(1f).semantics { heading() }
                )
                ActionIcon(AppSymbol.CLOSE, R.string.close, dismiss)
            }
            when {
                camera == false -> Column(
                    Modifier.fillMaxWidth().weight(1f),
                    verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
                    horizontalAlignment = Alignment.CenterHorizontally
                ) {
                    Text(
                        stringResource(R.string.camera_off),
                        style = KodosiType.body,
                        color = colors.ink,
                        textAlign = TextAlign.Center
                    )
                    KodosiButton(stringResource(R.string.open_settings), {
                        context.startActivity(
                            Intent(
                                Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                                Uri.fromParts("package", context.packageName, null)
                            )
                        )
                    })
                }

                failed -> ErrorNote(words.unavailable, Modifier.fillMaxWidth())

                else -> {
                    Box(
                        Modifier.fillMaxWidth().weight(
                            1f
                        ).clip(RoundedCornerShape(Radius.xl)).background(colors.terminal)
                    ) {
                        if (camera == true) {
                            CameraReader(
                                read = { code ->
                                    val accepted = accepts(code)
                                    when {
                                        done -> Unit

                                        accepted == null -> otherCode = true

                                        else -> {
                                            done = true
                                            found(accepted)
                                        }
                                    }
                                },
                                failed = { failed = true },
                                modifier = Modifier.fillMaxSize()
                            )
                        }
                    }
                    if (otherCode) {
                        ErrorNote(words.other, Modifier.fillMaxWidth())
                    } else {
                        Text(
                            words.hint,
                            style = KodosiType.callout,
                            color = colors.inkMuted,
                            textAlign = TextAlign.Center,
                            modifier = Modifier.fillMaxWidth()
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun CameraReader(read: (String) -> Unit, failed: () -> Unit, modifier: Modifier) {
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val reading by rememberUpdatedState(read)
    val failing by rememberUpdatedState(failed)
    var request by remember { mutableStateOf<SurfaceRequest?>(null) }
    LaunchedEffect(owner) {
        val provider = runCatching { ProcessCameraProvider.awaitInstance(context) }.getOrNull()
        val selector = listOf(CameraSelector.DEFAULT_BACK_CAMERA, CameraSelector.DEFAULT_FRONT_CAMERA)
            .firstOrNull { provider?.hasCamera(it) == true }
        if (provider == null || selector == null) {
            failing()
            return@LaunchedEffect
        }
        val main = ContextCompat.getMainExecutor(context)
        val decoding = Executors.newSingleThreadExecutor()
        val reader = QRCodeReader()
        val preview = Preview.Builder().build().apply { setSurfaceProvider { request = it } }
        val analysis = ImageAnalysis.Builder()
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .setResolutionSelector(
                ResolutionSelector.Builder()
                    .setResolutionStrategy(
                        ResolutionStrategy(Size(1280, 720), ResolutionStrategy.FALLBACK_RULE_CLOSEST_HIGHER_THEN_LOWER)
                    )
                    .build()
            )
            .build()
            .apply {
                setAnalyzer(decoding) { image ->
                    image.use { qrText(reader, it) }?.let { code -> main.execute { reading(code) } }
                }
            }
        try {
            provider.bindToLifecycle(owner, selector, preview, analysis)
            awaitCancellation()
        } catch (failure: IllegalArgumentException) {
            failing()
        } catch (failure: IllegalStateException) {
            failing()
        } finally {
            provider.unbind(preview, analysis)
            analysis.clearAnalyzer()
            decoding.shutdown()
        }
    }
    request?.let { CameraXViewfinder(surfaceRequest = it, modifier = modifier) }
}

private fun qrText(reader: QRCodeReader, image: ImageProxy): String? {
    val luminance = image.planes[0]
    val bytes = ByteArray(luminance.buffer.remaining()).also(luminance.buffer::get)
    val source =
        PlanarYUVLuminanceSource(bytes, luminance.rowStride, image.height, 0, 0, image.width, image.height, false)
    return try {
        reader.decode(BinaryBitmap(HybridBinarizer(source))).text
    } catch (_: ReaderException) {
        null
    } finally {
        reader.reset()
    }
}
