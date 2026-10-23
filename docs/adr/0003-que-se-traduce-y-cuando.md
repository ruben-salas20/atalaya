# 0003. Títulos siempre; texto completo al abrir o guardar

- **Estado:** aceptada
- **Fecha:** 2026-10-23

## Contexto

Rubén no lee inglés. Traducir solo lo guardado ahorraría GPU, pero la lista llegaría en inglés y no sabría qué guardar.

## Decisión

| Qué | Cuándo se traduce |
|---|---|
| Títulos de artículos y de resultados de búsqueda | Siempre, al llegar |
| Texto completo de un artículo, o resumen de un paper | Al abrirlo o al guardarlo |
| Lo ya traducido | Nunca otra vez. Se guarda en el servicio |
| Lo que ya está en español | Nunca. El servicio detecta el idioma |

Las apps no tienen ajustes de traducción.

## Consecuencias

- La primera vez que se abre un artículo hay una espera de unos segundos, con el aviso «Traduciendo…».
- La GPU solo trabaja en lo que interesa.
