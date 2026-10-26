package dev.rubensalas.atalaya.ui.article

import org.jsoup.Jsoup
import org.jsoup.nodes.Element
import org.jsoup.nodes.Node
import org.jsoup.nodes.TextNode

// El HTML del artículo se convierte en bloques que la pantalla pinta con
// Compose. Así el texto sigue el tema, los colores y el tamaño de letra de la
// app, y conserva enlaces, listas e imágenes (T5).

/** Texto con estilos. Los tramos usan posiciones de [text]. */
data class RichText(val text: String, val spans: List<Span> = emptyList())

data class Span(val start: Int, val end: Int, val style: SpanStyleKind)

sealed interface SpanStyleKind {
    data object Bold : SpanStyleKind
    data object Italic : SpanStyleKind
    data object Code : SpanStyleKind
    data class Link(val url: String) : SpanStyleKind

    /** Párrafo que aún no está traducido: en la versión parcial, se pinta en gris claro (ADR 0015). */
    data object Pending : SpanStyleKind

    /** Fórmula en línea: su imagen SVG, sus medidas en ex y su LaTeX. Ocupa un [FORMULA_PLACEHOLDER]. */
    data class Formula(val url: String, val size: FormulaSize, val latex: String) : SpanStyleKind
}

/** La clase que el servicio pone a cada párrafo que falta por traducir (ADR 0015). */
const val PENDING_CLASS = "atalaya-pendiente"

/** Una celda de tabla. [header] es true en las de cabecera (`th`). */
data class TableCell(val text: RichText, val header: Boolean = false)

sealed interface Block {
    data class Paragraph(val text: RichText) : Block
    data class Heading(val level: Int, val text: RichText) : Block
    data class Quote(val text: RichText) : Block

    /** Un elemento de lista. [marker] es «•» o «3.»; va vacío en el segundo párrafo de un mismo elemento. */
    data class ListItem(val marker: String, val depth: Int, val text: RichText) : Block
    data class Image(val url: String, val alt: String, val caption: RichText? = null) : Block

    /** Fórmula aparte, centrada en su línea (ADR 0010). */
    data class Formula(val url: String, val size: FormulaSize, val latex: String) : Block

    /** Tabla: filas de celdas. Si es ancha, se desplaza de lado. */
    data class Table(val rows: List<List<TableCell>>) : Block
    data class Code(val text: String) : Block
    data object Rule : Block
}

/** Convierte el HTML de un artículo en bloques. [baseUrl] resuelve los enlaces relativos. */
fun parseArticleHtml(html: String, baseUrl: String = ""): List<Block> {
    val body = Jsoup.parseBodyFragment(html, baseUrl).body()
    return HtmlWalker().apply { walkChildren(body) }.finish()
}

private val SKIPPED = setOf("script", "style", "noscript", "svg", "button", "form", "input", "select", "textarea", "template", "source")
private val CONTAINERS = setOf(
    "p", "div", "section", "article", "header", "footer", "main", "aside", "nav", "center", "address",
    "figcaption", "dl", "dt", "dd", "details", "summary", "hgroup", "body",
)
private val WHITESPACE = Regex("\\s+")

private class HtmlWalker {
    private val blocks = mutableListOf<Block>()
    private val text = StringBuilder()
    private val spans = mutableListOf<Span>()

    /** Estilos abiertos y dónde empiezan en [text]. */
    private val open = mutableListOf<OpenSpan>()

    /** Qué bloque sale al cerrar un párrafo: el de dentro manda. */
    private val contexts = ArrayDeque<Context>()
    private var listDepth = 0

    private class OpenSpan(val style: SpanStyleKind, var start: Int)

    private sealed interface Context {
        data class Heading(val level: Int) : Context
        data object Quote : Context
        class Item(val marker: String, val depth: Int, var used: Boolean = false) : Context
    }

    fun finish(): List<Block> {
        flush()
        return blocks
    }

    fun walkChildren(element: Element) {
        for (child in element.childNodes()) walk(child)
    }

    private fun walk(node: Node) {
        when (node) {
            is TextNode -> appendText(node.wholeText)
            is Element -> walkElement(node)
        }
    }

    private fun walkElement(el: Element) {
        when (val tag = el.normalName()) {
            in SKIPPED -> Unit
            "br" -> lineBreak()
            "hr" -> {
                flush()
                blocks += Block.Rule
            }
            "img" -> if (el.hasClass(FORMULA_CLASS)) formula(el) else image(el, caption = null)
            "figure", "picture" -> figure(el)
            "iframe", "video", "audio", "embed", "object" -> embed(el)
            "pre" -> {
                flush()
                val code = el.wholeText().trim('\n')
                if (code.isNotBlank()) blocks += Block.Code(code)
            }
            "h1", "h2", "h3", "h4", "h5", "h6" -> inContext(Context.Heading(tag[1].digitToInt())) { walkChildren(el) }
            "blockquote" -> inContext(Context.Quote) { walkChildren(el) }
            "ul", "ol" -> list(el, ordered = tag == "ol")
            "li" -> inContext(Context.Item("•", listDepth.coerceAtLeast(1) - 1)) { walkChildren(el) }
            "table" -> table(el)
            "b", "strong" -> styled(el, SpanStyleKind.Bold)
            "i", "em", "cite", "dfn" -> styled(el, SpanStyleKind.Italic)
            "code", "kbd", "samp", "tt" -> styled(el, SpanStyleKind.Code)
            "span" -> if (el.hasClass(PENDING_CLASS)) styled(el, SpanStyleKind.Pending) else walkChildren(el)
            "a" -> {
                val href = el.absUrl("href").ifBlank { el.attr("href") }
                if (href.isBlank() || href.startsWith("#") || href.startsWith("javascript:")) {
                    walkChildren(el)
                } else {
                    styled(el, SpanStyleKind.Link(href))
                }
            }
            in CONTAINERS -> {
                flush()
                walkChildren(el)
                flush()
            }
            else -> walkChildren(el)
        }
    }

    private fun styled(el: Element, style: SpanStyleKind) {
        val span = OpenSpan(style, text.length)
        open += span
        walkChildren(el)
        open.remove(span)
        if (text.length > span.start) spans += Span(span.start, text.length, style)
    }

    private fun inContext(context: Context, body: () -> Unit) {
        flush()
        contexts.addLast(context)
        body()
        flush()
        contexts.removeLast()
    }

    private fun list(el: Element, ordered: Boolean) {
        flush()
        listDepth++
        var number = el.attr("start").toIntOrNull() ?: 1
        for (child in el.childNodes()) {
            if (child is Element && child.normalName() == "li") {
                val marker = if (ordered) "$number." else if (listDepth > 1) "◦" else "•"
                number++
                inContext(Context.Item(marker, listDepth - 1)) { walkChildren(child) }
            } else {
                walk(child)
            }
        }
        flush()
        listDepth--
    }

    private fun table(el: Element) {
        flush()
        // Solo las filas de esta tabla: las de una tabla anidada van dentro de su celda.
        val rows = el.select("tr")
            .filter { row -> row.parents().firstOrNull { it.normalName() == "table" } == el }
            .map { row ->
                row.children().filter { it.normalName() == "td" || it.normalName() == "th" }
                    .map { cell -> TableCell(inlineText(cell) ?: RichText(""), header = cell.normalName() == "th") }
            }
            .filter { it.isNotEmpty() }
        if (rows.isNotEmpty()) blocks += Block.Table(rows)
    }

    private fun figure(el: Element) {
        // Las webs que cargan las imágenes tarde repiten cada una dentro de <noscript>.
        val images = el.select("img").filter { img ->
            img.parents().none { it.normalName() == "noscript" } && !img.hasClass(FORMULA_CLASS)
        }
        if (images.isEmpty()) {
            flush()
            walkChildren(el)
            flush()
            return
        }
        val caption = el.selectFirst("figcaption")?.let { inlineText(it) }
        images.forEachIndexed { i, img -> image(img, caption.takeIf { i == images.lastIndex }) }
        // Lo que no es imagen ni pie (un enlace de créditos, por ejemplo) sigue como texto.
        flush()
        for (child in el.children()) {
            val holdsImage = child.select("img").any { !it.hasClass(FORMULA_CLASS) }
            if (child.normalName() !in setOf("img", "picture", "figcaption", "source") && !holdsImage) walk(child)
        }
        flush()
    }

    private fun formula(img: Element) {
        val src = img.absUrl("src").ifBlank { img.attr("src") }
        if (src.isBlank()) return
        val size = parseFormulaStyle(img.attr("style"))
        val latex = img.attr("alt")
        if (img.hasClass(FORMULA_BLOCK_CLASS)) {
            flush()
            blocks += Block.Formula(src, size, latex)
        } else {
            val start = text.length
            text.append(FORMULA_PLACEHOLDER)
            spans += Span(start, text.length, SpanStyleKind.Formula(src, size, latex))
        }
    }

    private fun image(img: Element, caption: RichText?) {
        val src = img.absUrl("src").ifBlank { img.absUrl("data-src") }.ifBlank { img.attr("src") }
        // Los píxeles de seguimiento miden 1×1.
        if (src.isBlank() || src.startsWith("data:") || img.attr("width") == "1" || img.attr("height") == "1") return
        flush()
        blocks += Block.Image(src, img.attr("alt").trim(), caption?.takeIf { it.text.isNotBlank() })
    }

    private fun embed(el: Element) {
        val src = el.absUrl("src").ifBlank { el.selectFirst("source")?.absUrl("src").orEmpty() }
        flush()
        if (src.isBlank()) return
        val label = when (el.normalName()) {
            "audio" -> "Escuchar el audio"
            "video" -> "Ver el vídeo"
            else -> "Ver el contenido incrustado"
        }
        blocks += Block.Paragraph(RichText(label, listOf(Span(0, label.length, SpanStyleKind.Link(src)))))
    }

    private fun appendText(raw: String) {
        var s = raw.replace(WHITESPACE, " ")
        if (text.isEmpty() || text.last() == ' ' || text.last() == '\n') s = s.trimStart(' ')
        text.append(s)
    }

    private fun lineBreak() {
        while (text.isNotEmpty() && text.last() == ' ') text.setLength(text.length - 1)
        text.append('\n')
    }

    private fun flush() {
        for (o in open) {
            if (text.length > o.start) spans += Span(o.start, text.length, o.style)
        }
        val rich = trimmed(text.toString(), spans.toList())
        text.setLength(0)
        spans.clear()
        for (o in open) o.start = 0
        if (rich.text.isBlank()) return
        blocks += when (val ctx = contexts.lastOrNull()) {
            is Context.Heading -> Block.Heading(ctx.level, rich)
            Context.Quote -> Block.Quote(rich)
            is Context.Item -> Block.ListItem(if (ctx.used) "" else ctx.marker, ctx.depth, rich).also { ctx.used = true }
            null -> Block.Paragraph(rich)
        }
    }
}

/** El texto de un elemento como un solo bloque de texto: sus párrafos, uno detrás de otro. Null si no tiene texto. */
private fun inlineText(el: Element): RichText? =
    HtmlWalker().apply { walkChildren(el) }.finish().flatMap(::textsOf).reduceOrNull(::joinLines)

/** Los textos de un bloque. Una tabla anidada aporta el de todas sus celdas. */
private fun textsOf(block: Block): List<RichText> = when (block) {
    is Block.Paragraph -> listOf(block.text)
    is Block.Heading -> listOf(block.text)
    is Block.Quote -> listOf(block.text)
    is Block.ListItem -> listOf(block.text)
    is Block.Table -> block.rows.flatten().map { it.text }
    else -> emptyList()
}

private fun joinLines(a: RichText, b: RichText): RichText {
    val shift = a.text.length + 1
    return RichText(a.text + "\n" + b.text, a.spans + b.spans.map { it.copy(start = it.start + shift, end = it.end + shift) })
}

/** Quita los espacios y saltos de los bordes y ajusta los tramos. */
private fun trimmed(text: String, spans: List<Span>): RichText {
    val start = text.indexOfFirst { !it.isWhitespace() }
    if (start < 0) return RichText("")
    val end = text.indexOfLast { !it.isWhitespace() } + 1
    val out = text.substring(start, end)
    val kept = spans.mapNotNull { s ->
        val a = (s.start - start).coerceIn(0, out.length)
        val b = (s.end - start).coerceIn(0, out.length)
        if (b > a) Span(a, b, s.style) else null
    }
    return RichText(out, kept)
}
