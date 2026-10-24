package dev.rubensalas.atalaya.data.list

import java.util.Locale

/**
 * Pliega un texto para comparar búsquedas: minúsculas y sin tildes. Es el
 * mismo criterio que `atalaya_api::fold` en el servicio, con la misma tabla
 * de letras. No usa `Normalizer`: ese pliega más letras (como la «ő») y daría
 * otros resultados que el servicio.
 */
fun foldForSearch(text: String): String {
    val out = StringBuilder()
    text.codePoints().forEach { cp ->
        val lower = String(Character.toChars(cp)).lowercase(Locale.ROOT)
        for (c in lower) {
            if (c.code in 0x300..0x36F) continue // Marcas sueltas, como «o» + acento aparte.
            out.append(
                when (c) {
                    'á', 'à', 'ä', 'â', 'ã' -> 'a'
                    'é', 'è', 'ë', 'ê' -> 'e'
                    'í', 'ì', 'ï', 'î' -> 'i'
                    'ó', 'ò', 'ö', 'ô', 'õ' -> 'o'
                    'ú', 'ù', 'ü', 'û' -> 'u'
                    'ñ' -> 'n'
                    'ç' -> 'c'
                    'ý', 'ÿ' -> 'y'
                    else -> c
                },
            )
        }
    }
    return out.toString()
}

/** El texto a buscar, sin espacios sobrantes y plegado, o null si no hay nada que buscar. */
fun searchNeedle(text: String?): String? {
    val needle = foldForSearch(text?.trim().orEmpty())
    return needle.ifEmpty { null }
}

/** Sí si alguno de los títulos contiene el texto buscado, sin distinguir tildes ni mayúsculas. */
fun titleMatches(needle: String, vararg titles: String?): Boolean =
    titles.any { title -> title != null && foldForSearch(title).contains(needle) }
