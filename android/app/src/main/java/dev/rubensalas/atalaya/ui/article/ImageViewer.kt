package dev.rubensalas.atalaya.ui.article

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import coil3.compose.AsyncImage

/** Arrastre hacia abajo, en dp, que cierra el visor cuando la figura está a 1x. */
private val SWIPE_CLOSE_DP = 120.dp

/** Pausa, en ms, entre dos eventos de arrastre que cuenta como un gesto nuevo. */
private const val NEW_DRAG_GAP_MS = 150L

/**
 * La figura a pantalla completa: fondo negro, la figura centrada sobre su tarjeta
 * blanca, zoom y desplazamiento con los dedos, y el pie abajo sobre un degradado.
 * Se cierra con atrás, con el botón «Cerrar» o deslizando hacia abajo a 1x.
 */
@Composable
fun ImageViewer(image: Block.Image, onDismiss: () -> Unit) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        var transform by remember { mutableStateOf(ViewTransform.Identity) }
        var viewport by remember { mutableStateOf(Size.Zero) }
        var dragY by remember { mutableFloatStateOf(0f) }
        var dragX by remember { mutableFloatStateOf(0f) }
        var lastDragAt by remember { mutableStateOf(0L) }
        val closeThresholdPx = with(LocalDensity.current) { SWIPE_CLOSE_DP.toPx() }

        Column(
            Modifier
                .fillMaxSize()
                .background(Color.Black)
                .safeDrawingPadding(),
        ) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                IconButton(onClick = onDismiss) {
                    Icon(Icons.Filled.Close, contentDescription = "Cerrar", tint = Color.White)
                }
            }
            // El recorte hace que la figura ampliada no pise el botón ni el pie.
            Box(
                Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .padding(start = 12.dp, end = 12.dp)
                    .clipToBounds(),
            ) {
                Box(
                    Modifier
                        .matchParentSize()
                        .onSizeChanged { viewport = Size(it.width.toFloat(), it.height.toFloat()) }
                        .pointerInput(Unit) {
                            detectTapGestures(
                                onDoubleTap = { tap ->
                                    transform = doubleTapTransform(transform, viewport, tap)
                                },
                            )
                        }
                        .pointerInput(closeThresholdPx) {
                            detectTransformGestures { centroid, pan, zoomChange, _ ->
                                val now = System.currentTimeMillis()
                                if (now - lastDragAt > NEW_DRAG_GAP_MS) {
                                    dragX = 0f
                                    dragY = 0f
                                }
                                lastDragAt = now

                                transform = pinchTransform(transform, viewport, centroid, zoomChange, pan)
                                dragX += pan.x
                                dragY += pan.y
                                if (isSwipeDownToClose(transform.zoom, dragX, dragY, closeThresholdPx)) {
                                    onDismiss()
                                }
                            }
                        }
                        .graphicsLayer {
                            scaleX = transform.zoom
                            scaleY = transform.zoom
                            translationX = transform.offset.x
                            translationY = transform.offset.y
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    // La tarjeta se ajusta a la figura: sin proporción conocida aún, ocupa el ancho.
                    var ratio by remember { mutableFloatStateOf(0f) }
                    val viewportRatio = if (viewport.height > 0f) viewport.width / viewport.height else 1f
                    val card = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(Color.White)
                        .padding(8.dp)
                    AsyncImage(
                        model = image.url,
                        contentDescription = image.alt.ifBlank { null },
                        contentScale = ContentScale.Fit,
                        onSuccess = { state ->
                            val size = state.painter.intrinsicSize
                            if (size.width > 0f && size.height > 0f) ratio = size.width / size.height
                        },
                        modifier = if (ratio > 0f) {
                            card.aspectRatio(ratio, matchHeightConstraintsFirst = ratio < viewportRatio)
                        } else {
                            card.fillMaxWidth()
                        },
                    )
                }
            }
            image.caption?.let { caption ->
                // Pie en blanco, poco tamaño, sobre un degradado oscuro.
                val t = rich(caption, 13f, Color.White)
                Box(
                    Modifier
                        .fillMaxWidth()
                        .background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(alpha = 0.85f))))
                        .padding(start = 16.dp, end = 16.dp, top = 20.dp, bottom = 12.dp),
                ) {
                    Text(
                        t.text,
                        inlineContent = t.inline,
                        fontSize = 13.sp,
                        lineHeight = 18.sp,
                        color = Color.White,
                        textAlign = TextAlign.Start,
                    )
                }
            }
        }
    }
}
