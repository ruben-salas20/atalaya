package dev.rubensalas.atalaya.ui.article

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
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
import dev.rubensalas.atalaya.ui.theme.Atalaya

/** Un bloque del texto del artículo. [muted] pinta el original en gris mientras se traduce (pantalla 4). */
@Composable
fun ArticleBlock(block: Block, muted: Boolean, modifier: Modifier = Modifier) {
    val c = Atalaya.colors
    val color = if (muted) c.text3 else c.text
    when (block) {
        is Block.Paragraph -> Text(
            rich(block.text),
            fontSize = 17.sp,
            lineHeight = 27.sp,
            color = color,
            modifier = modifier.padding(bottom = 14.dp),
        )
        is Block.Heading -> Text(
            rich(block.text),
            fontSize = when (block.level) {
                1, 2 -> 21.sp
                3 -> 19.sp
                else -> 17.sp
            },
            lineHeight = 26.sp,
            fontWeight = FontWeight.Medium,
            color = color,
            modifier = modifier.padding(top = 6.dp, bottom = 10.dp),
        )
        is Block.Quote -> Row(modifier.padding(bottom = 14.dp)) {
            Box(Modifier.width(3.dp).heightIn(min = 24.dp).background(c.surface2))
            Text(
                rich(block.text),
                fontSize = 17.sp,
                lineHeight = 27.sp,
                fontStyle = FontStyle.Italic,
                color = if (muted) c.text3 else c.text2,
                modifier = Modifier.padding(start = 14.dp),
            )
        }
        is Block.ListItem -> Row(
            modifier.padding(start = (block.depth * 20).dp, bottom = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(block.marker, fontSize = 17.sp, lineHeight = 27.sp, color = c.text2, modifier = Modifier.width(22.dp))
            Text(rich(block.text), fontSize = 17.sp, lineHeight = 27.sp, color = color)
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
                Text(rich(it), fontSize = 13.sp, lineHeight = 18.sp, color = c.text3)
            }
        }
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

@Composable
private fun rich(text: RichText): AnnotatedString {
    val c = Atalaya.colors
    return remember(text, c) {
        buildAnnotatedString {
            append(text.text)
            for (span in text.spans) {
                when (val style = span.style) {
                    SpanStyleKind.Bold -> addStyle(SpanStyle(fontWeight = FontWeight.Bold), span.start, span.end)
                    SpanStyleKind.Italic -> addStyle(SpanStyle(fontStyle = FontStyle.Italic), span.start, span.end)
                    SpanStyleKind.Code -> addStyle(
                        SpanStyle(fontFamily = FontFamily.Monospace, background = c.surface),
                        span.start,
                        span.end,
                    )
                    // Lo que aún no está traducido, en gris claro (ADR 0015).
                    SpanStyleKind.Pending -> addStyle(SpanStyle(color = c.text3), span.start, span.end)
                    // Los enlaces abren el navegador del sistema.
                    is SpanStyleKind.Link -> addLink(
                        LinkAnnotation.Url(
                            style.url,
                            TextLinkStyles(SpanStyle(textDecoration = TextDecoration.Underline)),
                        ),
                        span.start,
                        span.end,
                    )
                }
            }
        }
    }
}
