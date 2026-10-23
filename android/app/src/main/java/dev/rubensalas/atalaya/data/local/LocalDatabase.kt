package dev.rubensalas.atalaya.data.local

import android.content.Context
import androidx.room.Dao
import androidx.room.Database
import androidx.room.Entity
import androidx.room.PrimaryKey
import androidx.room.Query
import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.room.Upsert
import dev.rubensalas.atalaya.data.api.Entry
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.TranslationState
import java.time.Instant

/**
 * Una fila de la lista guardada en el móvil (L8). [read] y [starred] son lo
 * que ve Rubén; si [readDirty] o [starredDirty], ese valor aún no llegó al
 * servicio y manda sobre lo que diga Miniflux hasta que se envíe.
 */
@Entity(tableName = "entries")
data class LocalEntry(
    @PrimaryKey val id: Id,
    val feedId: Id,
    val feedTitle: String,
    val title: String,
    val originalTitle: String,
    val titleTranslated: Boolean,
    val lang: String?,
    val url: String,
    val publishedAt: Long,
    val read: Boolean,
    val starred: Boolean,
    /** Cuándo vio el móvil este artículo leído por primera vez. Sirve para borrarlo a los 7 días. */
    val readSeenAt: Long?,
    val readDirty: Boolean = false,
    val starredDirty: Boolean = false,
) {
    val dirty: Boolean get() = readDirty || starredDirty

    fun toSummary() = EntrySummary(
        id, feedId, feedTitle, title, originalTitle, titleTranslated, lang, url,
        Instant.ofEpochMilli(publishedAt), read, starred,
    )
}

/** Texto completo de un artículo guardado o ya abierto (ADR 0003). */
@Entity(tableName = "contents")
data class LocalContent(
    @PrimaryKey val id: Id,
    val author: String?,
    val content: String,
    /** Nombre de [TranslationState]. */
    val translation: String,
    val translatedContent: String?,
    /** Rubén lo abrió. Si no, se guardó solo por estar en guardados. */
    val opened: Boolean,
) {
    val translationState: TranslationState
        get() = TranslationState.entries.firstOrNull { it.name == translation } ?: TranslationState.Failed

    fun toEntry(row: LocalEntry) = Entry(
        id = row.id, feedId = row.feedId, feedTitle = row.feedTitle, title = row.title,
        originalTitle = row.originalTitle, titleTranslated = row.titleTranslated, lang = row.lang,
        url = row.url, publishedAt = Instant.ofEpochMilli(row.publishedAt), read = row.read,
        starred = row.starred, author = author, content = content, translation = translationState,
        translatedContent = translatedContent,
    )
}

/** El menú lateral tal como llegó del servicio, en JSON. Hay una sola fila. */
@Entity(tableName = "sidebar")
data class LocalSidebar(@PrimaryKey val id: Int = 0, val json: String)

/**
 * Acceso a la base del móvil. Son consultas sencillas a propósito: los filtros
 * de la lista se hacen en Kotlin, sobre unos cientos de filas como mucho.
 */
@Dao
interface LocalDao {
    @Query("SELECT * FROM entries")
    suspend fun entries(): List<LocalEntry>

    @Query("SELECT * FROM entries WHERE id = :id")
    suspend fun entry(id: Id): LocalEntry?

    @Query("SELECT * FROM entries WHERE id IN (:ids)")
    suspend fun entries(ids: List<Id>): List<LocalEntry>

    @Query("SELECT * FROM entries WHERE readDirty = 1 OR starredDirty = 1")
    suspend fun dirtyEntries(): List<LocalEntry>

    @Upsert
    suspend fun upsertEntries(entries: List<LocalEntry>)

    @Query("DELETE FROM entries WHERE id IN (:ids)")
    suspend fun deleteEntries(ids: List<Id>)

    @Query("SELECT * FROM contents WHERE id = :id")
    suspend fun content(id: Id): LocalContent?

    /** Solo los campos ligeros: sin el HTML. */
    @Query("SELECT id, translation, opened FROM contents")
    suspend fun contentInfo(): List<ContentInfo>

    @Upsert
    suspend fun upsertContent(content: LocalContent)

    @Query("DELETE FROM contents WHERE id IN (:ids)")
    suspend fun deleteContents(ids: List<Id>)

    @Query("SELECT * FROM sidebar WHERE id = 0")
    suspend fun sidebar(): LocalSidebar?

    @Upsert
    suspend fun saveSidebar(sidebar: LocalSidebar)
}

data class ContentInfo(val id: Id, val translation: String, val opened: Boolean)

// Sin exportar el esquema: compilar.sh corre Gradle en ai-lab y no trae de
// vuelta los archivos generados, así que el JSON nunca llegaría al repo. Todo
// lo guardado se puede volver a bajar del servicio, por eso un cambio de
// esquema borra la base en vez de migrarla.
@Database(entities = [LocalEntry::class, LocalContent::class, LocalSidebar::class], version = 1, exportSchema = false)
abstract class LocalDatabase : RoomDatabase() {
    abstract fun dao(): LocalDao

    companion object {
        fun create(context: Context): LocalDatabase =
            Room.databaseBuilder(context, LocalDatabase::class.java, "atalaya.db")
                .fallbackToDestructiveMigration(dropAllTables = true)
                .build()
    }
}
