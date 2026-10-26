package dev.rubensalas.atalaya.ui.article

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.center
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

private val VIEWPORT = Size(1000f, 1000f)

class ImageZoomTest {
    @Test
    fun zoomStaysBetweenOneAndFive() {
        assertEquals(1f, clampZoom(0.4f), 0f)
        assertEquals(1f, clampZoom(1f), 0f)
        assertEquals(3.2f, clampZoom(3.2f), 0f)
        assertEquals(5f, clampZoom(5f), 0f)
        assertEquals(5f, clampZoom(9f), 0f)
    }

    @Test
    fun noPanAtOneX() {
        assertEquals(Offset.Zero, maxOffset(VIEWPORT, 1f))
        // Por componentes: coerceIn puede dejar -0.0, que no es igual a 0.0 como Offset.
        val clamped = clampOffset(Offset(300f, -200f), VIEWPORT, 1f)
        assertEquals(0f, clamped.x, 0f)
        assertEquals(0f, clamped.y, 0f)
    }

    @Test
    fun panLimitIsHalfTheExtraSize() {
        // A 2x sobran 500 px de ancho y de alto: 250 a cada lado.
        assertEquals(Offset(500f, 500f), maxOffset(VIEWPORT, 2f))
        assertEquals(Offset(2000f, 1000f), maxOffset(Size(1000f, 500f), 5f))
    }

    @Test
    fun offsetInsideTheEdgesIsKept() {
        assertEquals(Offset(100f, -200f), clampOffset(Offset(100f, -200f), VIEWPORT, 2f))
    }

    @Test
    fun offsetBeyondTheEdgesIsClamped() {
        assertEquals(Offset(500f, -500f), clampOffset(Offset(900f, -700f), VIEWPORT, 2f))
        assertEquals(Offset(-500f, 500f), clampOffset(Offset(-600f, 500f), VIEWPORT, 2f))
    }

    @Test
    fun pinchOutsideZoomRangeIsClamped() {
        val max = pinchTransform(ViewTransform(4f, Offset.Zero), VIEWPORT, Offset(500f, 500f), 3f, Offset.Zero)
        assertEquals(5f, max.zoom, 0f)

        val min = pinchTransform(ViewTransform(1.2f, Offset(10f, 10f)), VIEWPORT, Offset(500f, 500f), 0.1f, Offset.Zero)
        assertEquals(1f, min.zoom, 0f)
        assertEquals(Offset.Zero, min.offset)
    }

    @Test
    fun pinchKeepsThePointUnderTheFingersInPlace() {
        // Un pellizco de 1x a 2x sobre un punto a la derecha del centro: ese punto no se mueve.
        val centroid = Offset(700f, 500f)
        val after = pinchTransform(ViewTransform.Identity, VIEWPORT, centroid, 2f, Offset.Zero)
        assertEquals(2f, after.zoom, 0f)
        // Antes, el punto era el centroide; después sigue en centro + offset + zoom * (punto - centro).
        val contentPoint = (centroid - VIEWPORT.center) / 1f
        val screenAfter = VIEWPORT.center + after.offset + contentPoint * after.zoom
        assertEquals(centroid.x, screenAfter.x, 0.01f)
        assertEquals(centroid.y, screenAfter.y, 0.01f)
    }

    @Test
    fun pinchPanMovesTheImageAndStaysInBounds() {
        val zoomed = ViewTransform(2f, Offset.Zero)
        val moved = pinchTransform(zoomed, VIEWPORT, VIEWPORT.center, 1f, Offset(-120f, 30f))
        assertEquals(Offset(-120f, 30f), moved.offset)

        val pushed = pinchTransform(zoomed, VIEWPORT, VIEWPORT.center, 1f, Offset(-900f, 0f))
        assertEquals(Offset(-500f, 0f), pushed.offset)
    }

    @Test
    fun doubleTapAtOneXZoomsToTwoAndHalfOnTheTappedPoint() {
        // Tocar el centro: amplía sin desplazar.
        val atCenter = doubleTapTransform(ViewTransform.Identity, VIEWPORT, VIEWPORT.center)
        assertEquals(2.5f, atCenter.zoom, 0f)
        assertEquals(Offset.Zero, atCenter.offset)

        // Tocar a (+200, +100) del centro: ese punto queda bajo el dedo.
        val tap = VIEWPORT.center + Offset(200f, 100f)
        val aside = doubleTapTransform(ViewTransform.Identity, VIEWPORT, tap)
        assertEquals(2.5f, aside.zoom, 0f)
        assertEquals(Offset(-300f, -150f), aside.offset)
        val screenAfter = VIEWPORT.center + aside.offset + (tap - VIEWPORT.center) * 2.5f
        assertEquals(tap.x, screenAfter.x, 0.01f)
        assertEquals(tap.y, screenAfter.y, 0.01f)
    }

    @Test
    fun doubleTapAtAnyCornerStaysWithinEdges() {
        val corner = doubleTapTransform(ViewTransform.Identity, VIEWPORT, Offset.Zero)
        assertEquals(2.5f, corner.zoom, 0f)
        assertEquals(Offset(750f, 750f), corner.offset)
    }

    @Test
    fun doubleTapWhenZoomedReturnsToOneX() {
        val back = doubleTapTransform(ViewTransform(2.5f, Offset(-300f, 40f)), VIEWPORT, Offset(10f, 10f))
        assertEquals(ViewTransform.Identity, back)
    }

    @Test
    fun swipeDownClosesOnlyAtOneXAndWhenVertical() {
        assertTrue(isSwipeDownToClose(1f, dragX = 10f, dragY = 200f, thresholdPx = 120f))
        // Ampliada no cierra: el arrastre es para desplazar.
        assertFalse(isSwipeDownToClose(2f, dragX = 0f, dragY = 200f, thresholdPx = 120f))
        // Por debajo del umbral no cierra.
        assertFalse(isSwipeDownToClose(1f, dragX = 0f, dragY = 100f, thresholdPx = 120f))
        // Un arrastre más horizontal que vertical no cierra.
        assertFalse(isSwipeDownToClose(1f, dragX = 150f, dragY = 200f, thresholdPx = 120f))
        // Hacia arriba no cierra.
        assertFalse(isSwipeDownToClose(1f, dragX = 0f, dragY = -300f, thresholdPx = 120f))
    }
}
