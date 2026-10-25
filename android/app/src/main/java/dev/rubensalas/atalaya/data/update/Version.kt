package dev.rubensalas.atalaya.data.update

/**
 * Versión `X.Y.Z` (ADR 0012). Se compara por números: 0.10.0 es mayor que 0.9.0.
 */
data class Version(val major: Int, val minor: Int, val patch: Int) : Comparable<Version> {
    override fun compareTo(other: Version): Int =
        compareValuesBy(this, other, Version::major, Version::minor, Version::patch)

    override fun toString(): String = "$major.$minor.$patch"

    companion object {
        private val CORE = Regex("""(\d+)\.(\d+)\.(\d+)""")

        /**
         * Lee `v0.2.1` o `0.2.1`. Se quita la `v` y cualquier sufijo tras `-`
         * (`v0.2.1-rc1` es 0.2.1). Devuelve null si lo que queda no es `X.Y.Z` numérico.
         */
        fun parse(text: String): Version? {
            val core = text.trim().removePrefix("v").substringBefore('-')
            val match = CORE.matchEntire(core) ?: return null
            val (major, minor, patch) = match.destructured.toList().map { it.toIntOrNull() ?: return null }
            return Version(major, minor, patch)
        }
    }
}
