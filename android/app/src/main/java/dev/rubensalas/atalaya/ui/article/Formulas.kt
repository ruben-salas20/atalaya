package dev.rubensalas.atalaya.ui.article

// Las fórmulas del paper completo llegan como imágenes SVG del servicio (ADR
// 0010). Estas funciones leen sus medidas y pintan su color; no dependen de
// Compose, así que se prueban sin el móvil.

/**
 * Medidas de una fórmula, en `ex` (la altura de la «x» de la letra), como las
 * pone el servicio en el `style` de la imagen. [alignEx] es el `vertical-align`:
 * negativo, la fórmula baja por debajo de la línea de base.
 */
data class FormulaSize(val widthEx: Float, val heightEx: Float, val alignEx: Float)

/**
 * Cuántos em mide 1 ex. CSS no fija el valor: depende de la fuente. 0,5 em es
 * el que usan las fuentes habituales y el que supone AndroidSVG al leer el SVG.
 */
const val EX_IN_EM = 0.5f

/** Clase de toda fórmula del servicio. */
const val FORMULA_CLASS = "atalaya-formula"

/** Clase extra de las fórmulas aparte, que van centradas en su propia línea. */
const val FORMULA_BLOCK_CLASS = "atalaya-formula-bloque"

/** Carácter que ocupa una fórmula en línea dentro del texto. No es un espacio. */
const val FORMULA_PLACEHOLDER = '￼'

/** Lo que se usa si el `style` no trae una medida. */
val DEFAULT_FORMULA_SIZE = FormulaSize(widthEx = 2f, heightEx = 2f, alignEx = 0f)

/** Lee `width`, `height` y `vertical-align` de un atributo `style`. Lo que falte o no esté en ex queda por defecto. */
fun parseFormulaStyle(style: String): FormulaSize {
    val declared = style.split(';').mapNotNull { decl ->
        val colon = decl.indexOf(':')
        if (colon < 0) null else decl.substring(0, colon).trim().lowercase() to decl.substring(colon + 1).trim()
    }.toMap()
    fun ex(property: String): Float? = declared[property]?.let(::parseEx)
    return FormulaSize(
        widthEx = ex("width") ?: DEFAULT_FORMULA_SIZE.widthEx,
        heightEx = ex("height") ?: DEFAULT_FORMULA_SIZE.heightEx,
        alignEx = ex("vertical-align") ?: DEFAULT_FORMULA_SIZE.alignEx,
    )
}

/** «5.429ex» → 5.429. Null si el valor no está en ex. */
fun parseEx(value: String): Float? {
    val v = value.trim()
    if (!v.endsWith("ex")) return null
    return v.dropLast(2).toFloatOrNull()
}

/**
 * Cambia `currentColor` por [hexColor] en un SVG. MathJax pinta todo con ese
 * color, así que el SVG toma el del texto del tema y se ve en el claro y en el oscuro.
 */
fun recolorSvg(svg: String, hexColor: String): String = svg.replace(CURRENT_COLOR, hexColor)

private val CURRENT_COLOR = Regex("currentColor", RegexOption.IGNORE_CASE)

/** Color ARGB de Compose a `#RRGGBB`, sin el alfa. */
fun svgHexColor(argb: Int): String = "#%06X".format(argb and 0xFFFFFF)
