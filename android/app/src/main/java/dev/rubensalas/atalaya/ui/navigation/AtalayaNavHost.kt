package dev.rubensalas.atalaya.ui.navigation

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.toRoute
import dev.rubensalas.atalaya.AppContainer
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.ui.article.ArticleScreen
import dev.rubensalas.atalaya.ui.feeds.AddFeedScreen
import dev.rubensalas.atalaya.ui.feeds.EditFeedScreen
import dev.rubensalas.atalaya.ui.feeds.FeedsScreen
import dev.rubensalas.atalaya.ui.list.ListScreen
import dev.rubensalas.atalaya.ui.settings.SettingsScreen
import kotlinx.serialization.Serializable

// Cada pantalla vive en su paquete y recibe aquí sus callbacks de navegación.
// Las de lectura son del paso 3.2 y 3.3; las de feeds, del 3.4.

@Serializable object ListRoute
@Serializable data class ArticleRoute(val id: Id)
@Serializable object SettingsRoute
@Serializable object FeedsRoute
@Serializable object AddFeedRoute
@Serializable data class EditFeedRoute(val id: Id)

@Composable
fun AtalayaNavHost(container: AppContainer) {
    val nav = rememberNavController()
    // Sin clave guardada, la app abre en los ajustes.
    var start by remember { mutableStateOf<Any?>(null) }
    LaunchedEffect(Unit) {
        start = if (container.settings.current().apiKey.isBlank()) SettingsRoute else ListRoute
    }
    val startRoute = start ?: return

    NavHost(navController = nav, startDestination = startRoute) {
        composable<ListRoute> {
            ListScreen(
                container = container,
                onOpenArticle = { nav.navigate(ArticleRoute(it)) },
                onOpenSettings = { nav.navigate(SettingsRoute) },
                onOpenFeeds = { nav.navigate(FeedsRoute) },
            )
        }
        composable<ArticleRoute> {
            ArticleScreen(container = container, id = it.toRoute<ArticleRoute>().id, onBack = { nav.popBackStack() })
        }
        composable<SettingsRoute> {
            SettingsScreen(
                container = container,
                onBack = {
                    if (!nav.popBackStack()) {
                        nav.navigate(ListRoute) { popUpTo(SettingsRoute) { inclusive = true } }
                    }
                },
            )
        }
        composable<FeedsRoute> {
            FeedsScreen(
                container = container,
                onBack = { nav.popBackStack() },
                onAddFeed = { nav.navigate(AddFeedRoute) },
                onEditFeed = { nav.navigate(EditFeedRoute(it)) },
            )
        }
        composable<AddFeedRoute> {
            AddFeedScreen(container = container, onDone = { nav.popBackStack() })
        }
        composable<EditFeedRoute> {
            EditFeedScreen(container = container, id = it.toRoute<EditFeedRoute>().id, onDone = { nav.popBackStack() })
        }
    }
}
