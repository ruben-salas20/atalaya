# 0011. gpui-base 0.8.0 va parcheado en `vendor/` hasta que se publique el arreglo

- **Estado:** aceptada
- **Fecha:** 2026-10-23

## Contexto

En la app de PC, los artículos salían con cajas grises en lugar de texto y con huecos negros. Pasaba en los párrafos y elementos de lista con código en línea, y en los que mezclan texto con una imagen.

La causa está en `gpui-base` 0.8.0, la base del visor de texto de `gpui-component`. Esos párrafos se pintan con un «flujo en línea» que guarda su medida entre fotogramas bajo un id. El id sale de la posición del párrafo en el texto Markdown. El HTML no tiene esas posiciones, así que todos los párrafos de un artículo reciben el mismo id, `p`, y cada uno pinta la medida de otro. Con Markdown no pasa. Se reprodujo con un programa mínimo, sin código de Atalaya.

La 0.8.0 es la última versión publicada (2026-10-19). Se valoraron tres opciones:

- Parchear la biblioteca en el repo.
- Convertir el HTML a Markdown en la app. Puede perder tablas, pies de figura y HTML poco común.
- Que el servicio mande Markdown. Cambia el contrato de la API.

## Decisión

Una copia de `gpui-base` 0.8.0 vive en `vendor/gpui-base`, y el `Cargo.toml` del workspace la usa con `[patch.crates-io]`. El único cambio está en `src/text/node.rs`, marcado con «Atalaya:». Sin posición en el texto, el id del párrafo sale de la dirección de su `InlineState`, que es única y dura lo mismo que el párrafo.

## Consecuencias

- Los artículos HTML se ven bien: código con letra de ancho fijo, listas con código e imágenes junto al texto.
- El repo pesa unos 5 MB más.
- Al subir de versión `gpui-component` (ADR 0006), hay que mirar si el arreglo ya está publicado. Si está, se borran `vendor/gpui-base` y el `[patch.crates-io]`. Si no, hay que copiar la versión nueva y repetir el cambio.
- Se avisó al autor en https://github.com/longbridge/gpui-kit/issues/3668, para que el arreglo llegue a una versión publicada.
