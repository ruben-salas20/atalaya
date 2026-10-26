package dev.rubensalas.atalaya.ui.article

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.center
import kotlin.math.abs

// Lógica del visor de figuras a pantalla completa (issue #95), sin interfaz:
// el zoom va de 1x a 5x; el desplazamiento se queda dentro de los bordes de la
// tarjeta; el doble toque alterna entre 1x y 2,5x en el punto tocado.

/** Zoom mínimo: la figura entera, sin ampliar. */
const val MIN_ZOOM = 1f

/** Zoom máximo al pellizcar. */
const val MAX_ZOOM = 5f

/** Zoom al que va el doble toque desde 1x. */
const val DOUBLE_TAP_ZOOM = 2.5f

/**
 * Cómo se ve la tarjeta: cuánto está ampliada y cuánto se ha desplazado desde su centro,
 * en píxeles de pantalla. A 1x el desplazamiento es siempre cero.
 */
data class ViewTransform(val zoom: Float, val offset: Offset) {
    companion object {
        val Identity = ViewTransform(MIN_ZOOM, Offset.Zero)
    }
}

/** El zoom, entre [MIN_ZOOM] y [MAX_ZOOM]. */
fun clampZoom(zoom: Float): Float = zoom.coerceIn(MIN_ZOOM, MAX_ZOOM)

/**
 * Cuánto se puede desplazar la tarjeta a cada lado, con su centro en el centro del
 * visor: la mitad de lo que sobra al ampliarla. A 1x no se puede desplazar nada.
 */
fun maxOffset(viewport: Size, zoom: Float): Offset {
    val extra = clampZoom(zoom) - 1f
    return Offset(viewport.width * extra / 2f, viewport.height * extra / 2f)
}

/** Deja el desplazamiento dentro de los bordes de la tarjeta ampliada. */
fun clampOffset(offset: Offset, viewport: Size, zoom: Float): Offset {
    val max = maxOffset(viewport, zoom)
    return Offset(offset.x.coerceIn(-max.x, max.x), offset.y.coerceIn(-max.y, max.y))
}

/**
 * Un gesto de pellizco con [zoomFactor] y un desplazamiento [pan]. El punto que queda
 * bajo [centroid] se mantiene bajo los dedos mientras cambia el zoom.
 */
fun pinchTransform(
    current: ViewTransform,
    viewport: Size,
    centroid: Offset,
    zoomFactor: Float,
    pan: Offset,
): ViewTransform {
    val zoom = clampZoom(current.zoom * zoomFactor)
    val ratio = zoom / current.zoom
    val fromCenter = centroid - viewport.center
    val offset = fromCenter - (fromCenter - current.offset) * ratio + pan
    return ViewTransform(zoom, clampOffset(offset, viewport, zoom))
}

/**
 * Doble toque en [tap]: desde 1x, amplía a [DOUBLE_TAP_ZOOM] con el punto tocado
 * quieto; si ya está ampliada, vuelve a 1x centrada.
 */
fun doubleTapTransform(current: ViewTransform, viewport: Size, tap: Offset): ViewTransform {
    if (current.zoom > MIN_ZOOM) return ViewTransform.Identity
    val fromCenter = tap - viewport.center
    val ratio = DOUBLE_TAP_ZOOM / current.zoom
    val offset = fromCenter - (fromCenter - current.offset) * ratio
    return ViewTransform(DOUBLE_TAP_ZOOM, clampOffset(offset, viewport, DOUBLE_TAP_ZOOM))
}

/**
 * Deslizar hacia abajo para cerrar: solo a 1x, con un arrastre vertical que supera
 * [thresholdPx] y domina sobre el horizontal.
 */
fun isSwipeDownToClose(zoom: Float, dragX: Float, dragY: Float, thresholdPx: Float): Boolean =
    zoom <= MIN_ZOOM && dragY > thresholdPx && dragY > abs(dragX) * 2f
