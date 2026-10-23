# 0002. La primera versión traduce solo en ai-lab

- **Estado:** aceptada
- **Fecha:** 2026-10-23

## Contexto

Se valoró que las apps tuvieran también un traductor local (ML Kit en Android), elegible en los ajustes o como respaldo. `ai-lab` está siempre encendida.

## Decisión

Toda la traducción ocurre en `ai-lab`. Las apps no traducen. La traducción local queda como extra, para analizarla al final del plan.

## Consecuencias

- Si `ai-lab` se apaga, no llegan artículos nuevos de todos modos, porque Miniflux también vive ahí. El traductor local apenas ayudaría.
- Lo ya sincronizado se sigue leyendo sin conexión.
- Sin `ai-lab` no se puede buscar en arXiv.
- La app de PC se libra de un motor de traducción en Rust, que era su parte más difícil.
