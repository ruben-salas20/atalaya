package dev.rubensalas.atalaya.data.update

import java.io.File
import java.security.MessageDigest

// Línea de `sha256sum`: `<hex>  <nombre>`. Con `*` delante, modo binario: también vale.
private val SUM_LINE = Regex("""([0-9a-fA-F]{64})[ \t]+\*?(.+)""")

/** Busca la huella de [fileName] en el texto de un `SHA256SUMS`. Null si no está. */
fun expectedSha256(sums: String, fileName: String): String? =
    sums.lineSequence()
        .mapNotNull { SUM_LINE.matchEntire(it.trim()) }
        .firstOrNull { it.groupValues[2].trim() == fileName }
        ?.groupValues?.get(1)?.lowercase()

/** Huella SHA-256 de un archivo, en hexadecimal y en minúsculas. */
fun sha256Hex(file: File): String {
    val digest = MessageDigest.getInstance("SHA-256")
    file.inputStream().use { input ->
        val buffer = ByteArray(64 * 1024)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            digest.update(buffer, 0, read)
        }
    }
    return digest.digest().joinToString("") { "%02x".format(it.toInt() and 0xff) }
}
