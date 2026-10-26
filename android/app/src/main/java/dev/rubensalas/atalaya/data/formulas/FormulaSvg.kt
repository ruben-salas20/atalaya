package dev.rubensalas.atalaya.data.formulas

import coil3.ImageLoader
import coil3.decode.DataSource
import coil3.decode.ImageSource
import coil3.fetch.FetchResult
import coil3.fetch.Fetcher
import coil3.fetch.SourceFetchResult
import coil3.key.Keyer
import coil3.request.Options
import dev.rubensalas.atalaya.ui.article.recolorSvg
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import okio.Buffer
import okio.FileSystem
import java.io.IOException

/**
 * Una fórmula del paper completo: su SVG en el servicio y el color del texto
 * del tema, en `#RRGGBB` (ADR 0010). Va como modelo de Coil: el SVG se pide ya
 * con ese color.
 */
data class FormulaSvg(val url: String, val hexColor: String)

/** La clave de caché: el mismo SVG con otro color es otra imagen. */
object FormulaSvgKeyer : Keyer<FormulaSvg> {
    override fun key(data: FormulaSvg, options: Options): String = "formula|${data.url}|${data.hexColor}"
}

/** Coil pide las fórmulas con este fabricante, que descarga el SVG y lo recolorea. */
class FormulaSvgFetcherFactory(private val http: OkHttpClient) : Fetcher.Factory<FormulaSvg> {
    override fun create(data: FormulaSvg, options: Options, imageLoader: ImageLoader): Fetcher =
        FormulaSvgFetcher(data, http)
}

/**
 * Baja el SVG de la fórmula (sin clave: es público), cambia `currentColor` por
 * el color del tema y lo entrega a Coil como `image/svg+xml`.
 */
class FormulaSvgFetcher(private val data: FormulaSvg, private val http: OkHttpClient) : Fetcher {
    override suspend fun fetch(): FetchResult {
        val svg = withContext(Dispatchers.IO) {
            http.newCall(Request.Builder().url(data.url).build()).execute().use { response ->
                if (!response.isSuccessful) throw IOException("El servicio respondió ${response.code} para una fórmula")
                response.body.string()
            }
        }
        return SourceFetchResult(
            source = ImageSource(source = Buffer().writeUtf8(recolorSvg(svg, data.hexColor)), fileSystem = FileSystem.SYSTEM),
            mimeType = "image/svg+xml",
            dataSource = DataSource.NETWORK,
        )
    }
}
