# 0001. Servicio propio en ai-lab, con Miniflux dentro

- **Estado:** aceptada
- **Fecha:** 2026-10-23

## Contexto

Las apps necesitan feeds sincronizados entre el móvil y el PC, y artículos traducidos. Se valoraron tres caminos:

- Solo Miniflux, y que cada aparato traduzca.
- Un servicio propio que lo haga todo, incluida la descarga de feeds.
- Un servicio propio apoyado en Miniflux.

## Decisión

Un servicio propio en Rust, en `ai-lab`. Miniflux descarga los feeds. El servicio lee de Miniflux, traduce, busca en arXiv y es lo único con lo que hablan las apps.

## Consecuencias

- Miniflux resuelve lo difícil de los feeds: feeds mal formados, artículos repetidos y extraer el texto completo de una web.
- Se traduce una sola vez, y el móvil y el PC muestran la misma traducción.
- Las apps no necesitan un motor de traducción.
- Hay una pieza más que mantener.
- Si un día se quiere quitar Miniflux, las apps no cambian: solo hablan con el servicio.
