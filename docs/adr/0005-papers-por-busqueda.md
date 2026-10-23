# 0005. Papers por búsqueda, guardados en Miniflux

- **Estado:** aceptada
- **Fecha:** 2026-10-23

## Contexto

Las categorías de arXiv como feed traen cientos de papers al día: demasiados para leer y traducir. arXiv solo entiende búsquedas en inglés.

## Decisión

- Una sección «Buscar papers» consulta la API de arXiv. Rubén escribe en español y el servicio traduce la búsqueda al inglés.
- Filtros: dónde buscar (todo, título, autor o resumen) y cómo ordenar (más recientes o relevancia).
- Los papers guardados van a un feed «Papers guardados» de Miniflux, con su función «Import Entry». Existe desde Miniflux 2.2.16.
- Se traducen el título y el resumen. El paper completo queda como extra al final del plan.

## Consecuencias

- La API de arXiv responde en Atom y pide no pasar de una petición cada 3 segundos.
- Miniflux guarda el estado de leído y guardado de artículos y papers por igual.
