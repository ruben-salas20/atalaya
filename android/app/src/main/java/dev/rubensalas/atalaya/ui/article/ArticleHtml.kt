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
}

/** La clase que el servicio pone a cada párrafo que falta por traducir (ADR 0015). */
const val PENDING_CLASS = "atalaya-pendiente"

sealed interface Block {
    data class Paragraph(val text: RichText) : Block
    data class Heading(val level: Int, val text: RichText) : Block
    data class Quote(val text: RichText) : Block

    /** Un elemento de lista. [marker] es «•» o «3.»; va vacío en el segundo párrafo de un mismo elemento. */
    data class ListItem(val marker: String, val depth: Int, val text: RichText) : Block
    data class Image(val url: String, val alt: String, val caption: RichText? = null) : Block
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
            "img" -> image(el, caption = null)
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
        for (row in el.select("tr")) {
            var first = true
            for (cell in row.children()) {
                if (cell.normalName() != "td" && cell.normalName() != "th") continue
                if (!first) appendText(" · ")
                first = false
                walkChildren(cell)
            }
            flush()
        }
    }

    private fun figure(el: Element) {
        // Las webs que cargan las imágenes tarde repiten cada una dentro de <noscript>.
        val images = el.select("img").filter { img -> img.parents().none { it.normalName() == "noscript" } }
        if (images.isEmpty()) {
            flush()
            walkChildren(el)
            flush()
            return
        }
        val captionEl = el.selectFirst("figcaption")
        val caption = captionEl?.let { c ->
            HtmlWalker().apply { walkChildren(c) }.finish()
                .mapNotNull { (it as? Block.Paragraph)?.text }
                .reduceOrNull(::joinLines)
        }
        images.forEachIndexed { i, img -> image(img, caption.takeIf { i == images.lastIndex }) }
        // Lo que no es imagen ni pie (un enlace de créditos, por ejemplo) sigue como texto.
        flush()
        for (child in el.children()) {
            if (child.normalName() !in setOf("img", "picture", "figcaption", "source") && child.select("img").isEmpty()) walk(child)
        }
        flush()
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
