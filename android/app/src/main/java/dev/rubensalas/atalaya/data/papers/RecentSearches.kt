package dev.rubensalas.atalaya.data.papers

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import dev.rubensalas.atalaya.data.api.PaperField
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/** Una búsqueda reciente de papers (P6): el texto y dónde se buscó. */
@Serializable
data class RecentSearch(val q: String, val field: PaperField = PaperField.All)

/** Las búsquedas recientes, guardadas solo en el móvil. */
interface RecentSearches {
    /** La más reciente primero. */
    val searches: Flow<List<RecentSearch>>
    suspend fun add(search: RecentSearch)
    suspend fun remove(search: RecentSearch)
}

/**
 * Pone [search] la primera y quita la repetida, sin distinguir mayúsculas.
 * Guarda como mucho [max].
 */
fun addRecent(list: List<RecentSearch>, search: RecentSearch, max: Int = MAX_RECENT_SEARCHES): List<RecentSearch> {
    val clean = search.copy(q = search.q.trim())
    if (clean.q.isEmpty()) return list
    return (listOf(clean) + list.filterNot { it.sameAs(clean) }).take(max)
}

private fun RecentSearch.sameAs(other: RecentSearch) = field == other.field && q.equals(other.q, ignoreCase = true)

const val MAX_RECENT_SEARCHES = 10

private val Context.recentStore: DataStore<Preferences> by preferencesDataStore(name = "paper_searches")

class StoredRecentSearches(context: Context) : RecentSearches {
    private val store = context.applicationContext.recentStore
    private val json = Json { ignoreUnknownKeys = true }

    override val searches: Flow<List<RecentSearch>> = store.data.map { p -> decode(p[KEY]) }

    override suspend fun add(search: RecentSearch) {
        store.edit { it[KEY] = json.encodeToString(addRecent(decode(it[KEY]), search)) }
    }

    override suspend fun remove(search: RecentSearch) {
        store.edit { p -> p[KEY] = json.encodeToString(decode(p[KEY]).filterNot { it == search }) }
    }

    private fun decode(text: String?): List<RecentSearch> =
        text?.let { runCatching { json.decodeFromString<List<RecentSearch>>(it) }.getOrNull() }.orEmpty()

    private companion object {
        val KEY = stringPreferencesKey("recent")
    }
}
