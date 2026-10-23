# 0006. Kotlin para Android, Rust y GPUI para el PC, en un monorepo

- **Estado:** aceptada
- **Fecha:** 2026-10-23

## Contexto

Rubén quiere apps nativas en los dos aparatos, y GPUI en el PC. El móvil es un Samsung A35 con Android 16.

## Decisión

- **Android:** Kotlin y Jetpack Compose. Versión mínima, Android 14 (API 34). Va primero.
- **PC:** Rust y GPUI, con la biblioteca `gpui-component`. Va después.
- **Monorepo:** un workspace de Cargo en `crates/` con el servicio, la app de PC y un crate de tipos compartidos; la app de Android en `android/`.
- **Idioma del código:** nombres en inglés; comentarios, documentos y commits en español.

## Consecuencias

- El servicio y la app de PC comparten los tipos de la API y no pueden desincronizarse.
- La app de Android repite esos tipos en Kotlin.
- GPUI tiene poca documentación y su API cambia: hay que fijar su versión.
