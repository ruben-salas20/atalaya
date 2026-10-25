package dev.rubensalas.atalaya.data.update

import dev.rubensalas.atalaya.data.api.ApiException
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json

/** Nombres de los archivos que publica cada Release (ADR 0012). */
const val APK_NAME = "atalaya.apk"
const val SUMS_NAME = "SHA256SUMS"

/** La última Release publicada en GitHub, con lo que la app necesita. */
data class Release(
    /** La etiqueta tal cual, p. ej. `v0.2.1`. */
    val tag: String,
    val version: Version,
    /** Novedades escritas en la Release. Puede estar vacío. */
    val notes: String,
    val apkUrl: String,
    val sumsUrl: String,
)

@Serializable
private class ReleaseJson(
    @SerialName("tag_name") val tagName: String,
    val body: String? = null,
    val assets: List<AssetJson> = emptyList(),
)

@Serializable
private class AssetJson(
    val name: String,
    @SerialName("browser_download_url") val url: String,
)

private val releaseJson = Json { ignoreUnknownKeys = true }

/**
 * Lee el JSON de `GET .../releases/latest`. Lanza [ApiException] con un texto
 * en español si no sirve: JSON roto, etiqueta sin `X.Y.Z` o sin los dos archivos.
 */
fun parseRelease(text: String): Release {
    val data = try {
        releaseJson.decodeFromString<ReleaseJson>(text)
    } catch (e: SerializationException) {
        throw ApiException(null, "GitHub respondió algo que la app no entiende", e)
    }
    val version = Version.parse(data.tagName)
        ?: throw ApiException(null, "La última versión no tiene un número válido (${data.tagName})")
    val apkUrl = data.assets.firstOrNull { it.name == APK_NAME }?.url
    val sumsUrl = data.assets.firstOrNull { it.name == SUMS_NAME }?.url
    if (apkUrl == null || sumsUrl == null) {
        throw ApiException(null, "La versión ${data.tagName} no trae $APK_NAME o $SUMS_NAME")
    }
    return Release(data.tagName, version, data.body.orEmpty(), apkUrl, sumsUrl)
}
