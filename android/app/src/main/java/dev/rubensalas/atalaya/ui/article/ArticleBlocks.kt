package dev.rubensalas.atalaya.ui.article

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil3.compose.AsyncImage
import dev.rubensalas.atalaya.data.formulas.FormulaSvg
import dev.rubensalas.atalaya.ui.theme.Atalaya

/** Tamaño del texto corrido, en sp: sobre él se miden las fórmulas en ex. */
private const val BODY_SP = 17f

/** Ancho fijo de cada celda de tabla: así las columnas quedan alineadas entre filas (ADR 0010). */
private val TABLE_CELL_WIDTH = 150.dp

/** Un bloque del texto del artículo. [muted] pinta el original en gris mientras se traduce (pantalla 4). */
@Composable
fun ArticleBlock(block: Block, muted: Boolean, modifier: Modifier = Modifier) {
    val c = Atalaya.colors
    val color = if (muted) c.text3 else c.text
    when (block) {
        is Block.Paragraph -> {
            val t = rich(block.text, BODY_SP, color)
            Text(
                t.text,
                inlineContent = t.inline,
                fontSize = BODY_SP.sp,
                lineHeight = 27.sp,
                color = color,
                modifier = modifier.padding(bottom = 14.dp),
            )
        }
        is Block.Heading -> {
            val size = when (block.level) {
                1, 2 -> 21f
                3 -> 19f
                else -> 17f
            }
            val t = rich(block.text, size, color)
            Text(
                t.text,
                inlineContent = t.inline,
                fontSize = size.sp,
                lineHeight = 26.sp,
                fontWeight = FontWeight.Medium,
                color = color,
                modifier = modifier.padding(top = 6.dp, bottom = 10.dp),
            )
        }
        is Block.Quote -> {
            val quoteColor = if (muted) c.text3 else c.text2
            val t = rich(block.text, BODY_SP, quoteColor)
            Row(modifier.padding(bottom = 14.dp)) {
                Box(Modifier.width(3.dp).heightIn(min = 24.dp).background(c.surface2))
                Text(
                    t.text,
                    inlineContent = t.inline,
                    fontSize = BODY_SP.sp,
                    lineHeight = 27.sp,
                    fontStyle = FontStyle.Italic,
                    color = quoteColor,
                    modifier = Modifier.padding(start = 14.dp),
                )
            }
        }
        is Block.ListItem -> Row(
            modifier.padding(start = (block.depth * 20).dp, bottom = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            val t = rich(block.text, BODY_SP, color)
            Text(block.marker, fontSize = 17.sp, lineHeight = 27.sp, color = c.text2, modifier = Modifier.width(22.dp))
            Text(t.text, inlineContent = t.inline, fontSize = 17.sp, lineHeight = 27.sp, color = color)
        }
        is Block.Image -> Column(modifier.padding(bottom = 14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            AsyncImage(
                model = block.url,
                contentDescription = block.alt.ifBlank { null },
                contentScale = ContentScale.FillWidth,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 48.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(c.surface),
            )
            block.caption?.let {
                val t = rich(it, 13f, c.text3)
                Text(t.text, inlineContent = t.inline, fontSize = 13.sp, lineHeight = 18.sp, color = c.text3)
            }
        }
        is Block.Formula -> FormulaBlock(block, color, modifier.fillMaxWidth().padding(bottom = 14.dp))
        is Block.Table -> ArticleTable(block, color, modifier.padding(bottom = 14.dp))
        is Block.Code -> Text(
            block.text,
            fontFamily = FontFamily.Monospace,
            fontSize = 14.sp,
            lineHeight = 20.sp,
            color = color,
            softWrap = false,
            modifier = modifier
                .padding(bottom = 14.dp)
                .fillMaxWidth()
                .clip(RoundedCornerShape(8.dp))
                .background(c.surface)
                .horizontalScroll(rememberScrollState())
                .padding(12.dp),
        )
        Block.Rule -> HorizontalDivider(modifier.padding(vertical = 12.dp), thickness = 1.dp, color = c.divider)
    }
}

/** Fórmula aparte: centrada en su línea, con el tamaño que dice el servicio en ex (ADR 0010). */
@Composable
private fun FormulaBlock(block: Block.Formula, color: Color, modifier: Modifier) {
    val density = LocalDensity.current
    Box(modifier, contentAlignment = Alignment.Center) {
        FormulaImage(
            formula = FormulaSvg(block.url, svgColor(color)),
            latex = block.latex,
            modifier = Modifier.size(
                width = with(density) { (block.size.widthEx * EX_IN_EM * BODY_SP).sp.toDp() },
                height = with(density) { (block.size.heightEx * EX_IN_EM * BODY_SP).sp.toDp() },
            ),
        )
    }
}

/**
 * Tabla: una fila por línea, cada celda de ancho fijo. Si no cabe en la
 * pantalla, se desplaza de lado (ADR 0010).
 */
@Composable
private fun ArticleTable(table: Block.Table, color: Color, modifier: Modifier) {
    val c = Atalaya.colors
    Box(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .border(1.dp, c.divider, RoundedCornerShape(8.dp))
            .horizontalScroll(rememberScrollState()),
    ) {
        Column {
            for (row in table.rows) {
                Row(Modifier.height(IntrinsicSize.Min)) {
                    for (cell in row) {
                        val t = rich(cell.text, 14f, color)
                        Text(
                            t.text,
                            inlineContent = t.inline,
                            fontSize = 14.sp,
                            lineHeight = 19.sp,
                            fontWeight = if (cell.header) FontWeight.Medium else FontWeight.Normal,
                            color = color,
                            modifier = Modifier
                                // Un colspan de k mide k anchos de celda: las filas de datos cuadran con él.
                                .width(TABLE_CELL_WIDTH * cell.colspan)
                                .fillMaxHeight()
                                .background(if (cell.header) c.surface else c.bg)
                                .border(0.5.dp, c.divider)
                                .padding(8.dp),
                        )
                    }
                }
            }
        }
    }
}

/** Una fórmula en línea o aparte, pintada con Coil. [latex] es el texto alternativo. */
@Composable
private fun FormulaImage(formula: FormulaSvg, latex: String, modifier: Modifier) {
    AsyncImage(
        model = formula,
        contentDescription = latex.ifBlank { null },
        contentScale = ContentScale.FillBounds,
        modifier = modifier,
    )
}

/**
 * Texto listo para pintar: el texto con sus estilos y, por cada fórmula en
 * línea, el contenido que la pinta en su sitio (`InlineTextContent`).
 */
class RichContent(val text: AnnotatedString, val inline: Map<String, InlineTextContent>)

@Composable
private fun rich(text: RichText, fontSizeSp: Float, color: Color): RichContent {
    val c = Atalaya.colors
    val annotated = remember(text, c) {
        buildAnnotatedString {
            val formulas = formulaSpans(text)
            var cursor = 0
            formulas.forEachIndexed { i, f ->
                appendStyled(text, cursor, f.start, c.surface, c.text3)
                appendInlineContent(formulaId(i), (f.style as SpanStyleKind.Formula).latex)
                cursor = f.end
            }
            appendStyled(text, cursor, text.text.length, c.surface, c.text3)
        }
    }
    val inline = remember(text, fontSizeSp, color, c) {
        val formulas = formulaSpans(text)
        formulas.mapIndexed { i, f ->
            val formula = f.style as SpanStyleKind.Formula
            // Dentro de un trozo pendiente, la fórmula también va en gris.
            val tint = if (isPending(text, f)) c.text3 else color
            formulaId(i) to InlineTextContent(
                Placeholder(
                    width = (formula.size.widthEx * EX_IN_EM * fontSizeSp).sp,
                    height = (formula.size.heightEx * EX_IN_EM * fontSizeSp).sp,
                    placeholderVerticalAlign = PlaceholderVerticalAlign.AboveBaseline,
                ),
            ) {
                // La base de la imagen queda sobre la línea de base; el desplazamiento la baja lo que dice vertical-align.
                val shift = with(LocalDensity.current) { (-formula.size.alignEx * EX_IN_EM * fontSizeSp).sp.toDp() }
                FormulaImage(
                    formula = FormulaSvg(formula.url, svgColor(tint)),
                    latex = formula.latex,
                    modifier = Modifier.fillMaxSize().offset(y = shift),
                )
            }
        }.toMap()
    }
    return RichContent(annotated, inline)
}

/** Las fórmulas en línea de un texto, en orden. */
private fun formulaSpans(text: RichText): List<Span> =
    text.spans.filter { it.style is SpanStyleKind.Formula }.sortedBy { it.start }

private fun formulaId(index: Int): String = "formula-$index"

private fun isPending(text: RichText, formula: Span): Boolean =
    text.spans.any { it.style == SpanStyleKind.Pending && it.start <= formula.start && formula.end <= it.end }

/** El color que lleva el SVG: el ARGB del tema, en `#RRGGBB`. */
private fun svgColor(color: Color): String = svgHexColor(color.toArgb())

/** Escribe `text[from, to)` con sus estilos. Las fórmulas se escriben aparte, en [appendInlineContent]. */
private fun AnnotatedString.Builder.appendStyled(text: RichText, from: Int, to: Int, codeBackground: Color, pending: Color) {
    if (to <= from) return
    val base = length
    append(text.text.substring(from, to))
    for (span in text.spans) {
        if (span.style is SpanStyleKind.Formula) continue
        val s = maxOf(span.start, from)
        val e = minOf(span.end, to)
        if (e <= s) continue
        val a = base + s - from
        val b = base + e - from
        when (val style = span.style) {
            SpanStyleKind.Bold -> addStyle(SpanStyle(fontWeight = FontWeight.Bold), a, b)
            SpanStyleKind.Italic -> addStyle(SpanStyle(fontStyle = FontStyle.Italic), a, b)
            SpanStyleKind.Code -> addStyle(SpanStyle(fontFamily = FontFamily.Monospace, background = codeBackground), a, b)
            // Lo que aún no está traducido, en gris claro (ADR 0015).
            SpanStyleKind.Pending -> addStyle(SpanStyle(color = pending), a, b)
            // Los enlaces abren el navegador del sistema.
            is SpanStyleKind.Link -> addLink(
                LinkAnnotation.Url(style.url, TextLinkStyles(SpanStyle(textDecoration = TextDecoration.Underline))),
                a,
                b,
            )
            is SpanStyleKind.Formula -> Unit
        }
    }
}
