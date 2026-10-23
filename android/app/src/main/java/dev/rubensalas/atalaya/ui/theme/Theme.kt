package dev.rubensalas.atalaya.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Density
import dev.rubensalas.atalaya.R

/**
 * Paleta neutra de las maquetas (`docs/diseño/maquetas.html`): solo negro,
 * blanco y grises. Los nombres son los de sus variables CSS.
 */
@Immutable
data class AtalayaColors(
    val bg: Color,
    val surface: Color,
    val surface2: Color,
    val text: Color,
    val text2: Color,
    val text3: Color,
    val divider: Color,
    val accent: Color,
    val onAccent: Color,
    val scrim: Color,
)

val LightColors = AtalayaColors(
    bg = Color(0xFFFFFFFF), surface = Color(0xFFF4F4F4), surface2 = Color(0xFFE8E8E8),
    text = Color(0xFF111111), text2 = Color(0xFF555555), text3 = Color(0xFF8C8C8C),
    divider = Color(0xFFECECEC), accent = Color(0xFF111111), onAccent = Color(0xFFFFFFFF),
    scrim = Color(0x52000000),
)

val DarkColors = AtalayaColors(
    bg = Color(0xFF121212), surface = Color(0xFF1C1C1C), surface2 = Color(0xFF2A2A2A),
    text = Color(0xFFF1F1F1), text2 = Color(0xFFB0B0B0), text3 = Color(0xFF7C7C7C),
    divider = Color(0xFF242424), accent = Color(0xFFF1F1F1), onAccent = Color(0xFF111111),
    scrim = Color(0x8C000000),
)

val LocalAtalayaColors = staticCompositionLocalOf { LightColors }

/** Atajo: `Atalaya.colors.text2`. */
object Atalaya {
    val colors: AtalayaColors
        @Composable get() = LocalAtalayaColors.current
}

// Roboto va dentro de la app: la letra del sistema en Samsung es otra (PRD, Diseño).
private fun roboto(weight: Int) = Font(
    R.font.roboto,
    weight = FontWeight(weight),
    variationSettings = FontVariation.Settings(FontVariation.weight(weight)),
)

val Roboto = FontFamily(roboto(400), roboto(500), roboto(700))

private fun typography(): Typography {
    val base = Typography()
    return Typography(
        displayLarge = base.displayLarge.copy(fontFamily = Roboto),
        displayMedium = base.displayMedium.copy(fontFamily = Roboto),
        displaySmall = base.displaySmall.copy(fontFamily = Roboto),
        headlineLarge = base.headlineLarge.copy(fontFamily = Roboto),
        headlineMedium = base.headlineMedium.copy(fontFamily = Roboto),
        headlineSmall = base.headlineSmall.copy(fontFamily = Roboto),
        titleLarge = base.titleLarge.copy(fontFamily = Roboto),
        titleMedium = base.titleMedium.copy(fontFamily = Roboto),
        titleSmall = base.titleSmall.copy(fontFamily = Roboto),
        bodyLarge = base.bodyLarge.copy(fontFamily = Roboto),
        bodyMedium = base.bodyMedium.copy(fontFamily = Roboto),
        bodySmall = base.bodySmall.copy(fontFamily = Roboto),
        labelLarge = base.labelLarge.copy(fontFamily = Roboto),
        labelMedium = base.labelMedium.copy(fontFamily = Roboto),
        labelSmall = base.labelSmall.copy(fontFamily = Roboto),
    )
}

/**
 * Tema claro u oscuro según el sistema. [fontScale] es el ajuste «Tamaño de
 * letra» y se suma al del sistema.
 */
@Composable
fun AtalayaTheme(fontScale: Float = 1f, content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val c = if (dark) DarkColors else LightColors
    val scheme = (if (dark) darkColorScheme() else lightColorScheme()).copy(
        primary = c.accent, onPrimary = c.onAccent,
        secondary = c.text2, onSecondary = c.bg,
        background = c.bg, onBackground = c.text,
        surface = c.bg, onSurface = c.text,
        surfaceVariant = c.surface, onSurfaceVariant = c.text2,
        surfaceContainerLowest = c.bg, surfaceContainerLow = c.bg,
        surfaceContainer = c.surface, surfaceContainerHigh = c.surface,
        surfaceContainerHighest = c.surface2,
        secondaryContainer = c.surface2, onSecondaryContainer = c.text,
        outline = c.text3, outlineVariant = c.divider,
        inverseSurface = c.accent, inverseOnSurface = c.onAccent,
        scrim = c.scrim,
    )
    val density = LocalDensity.current
    CompositionLocalProvider(
        LocalAtalayaColors provides c,
        LocalDensity provides Density(density.density, density.fontScale * fontScale),
    ) {
        MaterialTheme(colorScheme = scheme, typography = typography(), content = content)
    }
}
