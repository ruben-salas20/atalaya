package dev.rubensalas.atalaya.ui.list

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitHorizontalTouchSlopOrCancellation
import androidx.compose.foundation.gestures.horizontalDrag
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.filled.BookmarkRemove
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.MarkEmailUnread
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.settings.SwipeAction
import dev.rubensalas.atalaya.ui.theme.Atalaya
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * Franja del borde izquierdo donde la fila no atiende el gesto: ahí se abre
 * el menú lateral (pantalla 2). Fuera de ella, deslizar a la derecha es de la fila.
 */
val DrawerEdge = 32.dp

/** Lo que hay que desplazar la fila para que el gesto cuente. */
private val Threshold = 96.dp

/** Lo más que se desplaza la fila. */
private val MaxOffset = 160.dp

/**
 * Fila que se desliza a los lados (L4). [left] es lo que hace deslizar hacia
 * la izquierda, y [right], hacia la derecha. Con [SwipeAction.None] ese lado
 * no se mueve y el gesto queda libre para el menú lateral.
 */
@Composable
fun SwipeRow(
    entry: EntrySummary,
    left: SwipeAction,
    right: SwipeAction,
    onSwipe: (SwipeAction) -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val c = Atalaya.colors
    val density = LocalDensity.current
    val haptic = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val offset = remember { Animatable(0f) }
    val currentLeft by rememberUpdatedState(left)
    val currentRight by rememberUpdatedState(right)
    val currentOnSwipe by rememberUpdatedState(onSwipe)
    val edgePx = with(density) { DrawerEdge.toPx() }
    val thresholdPx = with(density) { Threshold.toPx() }
    val maxPx = with(density) { MaxOffset.toPx() }

    val actions = listOf(left, right).filter { it != SwipeAction.None }.distinct()

    Box(
        modifier
            .semantics {
                customActions = actions.map { action ->
                    CustomAccessibilityAction(swipeLabel(action, entry)) {
                        onSwipe(action)
                        true
                    }
                }
            }
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    if (down.position.x < edgePx) return@awaitEachGesture
                    fun allowed(dx: Float) =
                        (dx < 0 && currentLeft != SwipeAction.None) || (dx > 0 && currentRight != SwipeAction.None)

                    var x = offset.value
                    // Sin consumir el gesto, el padre (la lista o el menú) puede quedárselo.
                    val drag = awaitHorizontalTouchSlopOrCancellation(down.id) { change, over ->
                        if (allowed(over)) {
                            change.consume()
                            x += over
                        }
                    } ?: return@awaitEachGesture

                    fun clamp(v: Float) = v.coerceIn(
                        if (currentLeft != SwipeAction.None) -maxPx else 0f,
                        if (currentRight != SwipeAction.None) maxPx else 0f,
                    )
                    x = clamp(x)
                    var armed = abs(x) >= thresholdPx
                    scope.launch { offset.snapTo(x) }
                    val completed = horizontalDrag(drag.id) { change ->
                        x = clamp(x + change.positionChange().x)
                        change.consume()
                        val nowArmed = abs(x) >= thresholdPx
                        if (nowArmed != armed) {
                            armed = nowArmed
                            if (armed) haptic.performHapticFeedback(HapticFeedbackType.GestureThresholdActivate)
                        }
                        scope.launch { offset.snapTo(x) }
                    }
                    if (completed && armed) currentOnSwipe(if (x < 0) currentLeft else currentRight)
                    scope.launch { offset.animateTo(0f) }
                }
            },
    ) {
        val x = offset.value
        if (x != 0f) {
            val action = if (x < 0) left else right
            val armed = abs(x) >= thresholdPx
            Row(
                Modifier.matchParentSize().background(c.surface2).padding(horizontal = 22.dp),
                horizontalArrangement = if (x < 0) Arrangement.End else Arrangement.Start,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val tint = if (armed) c.text else c.text3
                Icon(swipeIcon(action, entry), contentDescription = null, tint = tint, modifier = Modifier.size(18.dp))
                Text(
                    swipeLabel(action, entry),
                    fontSize = 14.sp,
                    fontWeight = FontWeight.Medium,
                    color = tint,
                    modifier = Modifier.padding(start = 8.dp),
                )
            }
        }
        Box(Modifier.offset { IntOffset(offset.value.roundToInt(), 0) }.background(c.bg)) {
            content()
        }
    }
}

/** Lo que dice el fondo de la fila: lo que pasará al soltar. */
fun swipeLabel(action: SwipeAction, entry: EntrySummary): String = when (action) {
    SwipeAction.MarkRead -> if (entry.read) "No leído" else "Leído"
    SwipeAction.Star -> if (entry.starred) "Quitar" else "Guardar"
    SwipeAction.None -> ""
}

private fun swipeIcon(action: SwipeAction, entry: EntrySummary): ImageVector = when (action) {
    SwipeAction.MarkRead -> if (entry.read) Icons.Filled.MarkEmailUnread else Icons.Filled.Check
    SwipeAction.Star -> if (entry.starred) Icons.Filled.BookmarkRemove else Icons.Filled.Bookmark
    SwipeAction.None -> Icons.Filled.Check
}
