# PRD de Atalaya

Atalaya es un lector RSS personal que traduce al español lo que llega en otros idiomas. Tiene una app de Android y una de PC. Este documento dice **qué** hace y **para quién**. El **porqué** de cada decisión técnica está en `docs/adr/`, y el **orden de trabajo**, en `docs/plan.md`.

Este documento solo cambia con la aprobación de Rubén.

## Problema

Rubén quiere seguir noticias de tecnología y papers científicos, pero no lee inglés. Los traductores de los navegadores funcionan página a página, envían el texto a terceros y no sirven para seguir muchas fuentes a la vez.

## Usuario

Una sola persona: Rubén. La app es personal y no se publica.

- Móvil: Samsung A35 con Android 16.
- PC: portátil con Omarchy (Arch Linux y Hyprland).
- Servidor: `ai-lab`, una torre con GPU RTX 3050 de 6 GB, siempre encendida y accesible por Tailscale.

## Objetivos

1. Leer en español noticias y papers publicados en inglés.
2. Encontrar papers concretos en arXiv sin escribir en inglés.
3. Tener lo mismo en el móvil y en el PC: feeds, leídos y guardados.
4. Que la traducción sea privada y gratis: el texto no sale de las máquinas de Rubén.

## Fuera de alcance

- Publicar la app en Google Play o compartirla con otras personas.
- Seguir categorías enteras de arXiv como feed (ver ADR 0005).
- Traducir dentro de las apps en la primera versión (ver ADR 0002).

## Requisitos

Cada requisito tiene un código para citarlo desde el plan.

### Lectura

| Código | Requisito |
|---|---|
| L1 | Las listas son Todos, Sin leer, Guardados y una por cada feed o carpeta |
| L2 | La lista es compacta: título traducido, fuente, hora y etiqueta del idioma original (`EN`) |
| L3 | Los artículos leídos se ven en gris |
| L4 | Deslizar a la izquierda marca como leído; a la derecha, guarda |
| L5 | El artículo se abre en español, con un selector para ver el original |
| L6 | Si la traducción no está lista, se ve el original con el aviso «Traduciendo…» y cambia solo al terminar |
| L7 | Si el feed solo trae un resumen, se muestra el artículo completo |
| L8 | Lo ya descargado se lee sin conexión |

### Traducción

| Código | Requisito |
|---|---|
| T1 | Del inglés al español. El chino llega más adelante |
| T2 | Los títulos se traducen siempre; el texto completo, al abrir o guardar (ver ADR 0003) |
| T3 | Lo que ya está en español no se traduce |
| T4 | Nada se traduce dos veces |
| T5 | La traducción conserva enlaces, listas e imágenes del artículo |

### Papers

| Código | Requisito |
|---|---|
| P1 | Buscar en arXiv escribiendo en español. La app muestra cómo se buscará en inglés |
| P2 | Filtrar dónde buscar (todo, título, autor o resumen) y ordenar (más recientes o relevancia) |
| P3 | Los resultados muestran título traducido, autores, fecha y categoría |
| P4 | Un paper abierto muestra el resumen traducido y enlaces a su página de arXiv y a su PDF |
| P5 | Los papers guardados aparecen en Guardados, sincronizados entre aparatos |
| P6 | Se recuerdan las búsquedas recientes |

### Feeds

| Código | Requisito |
|---|---|
| F1 | Añadir un feed pegando la dirección de la web. Si la web tiene varios feeds, se elige uno |
| F2 | Cada feed encontrado muestra su idioma y cuántos artículos publica al día |
| F3 | Elegir la carpeta y si se descarga el artículo completo |
| F4 | Editar nombre, carpeta y descarga completa; quitar el feed, con confirmación |
| F5 | Renombrar y borrar carpetas |
| F6 | Ver qué feeds fallan al actualizarse |

### Sincronización y ajustes

| Código | Requisito |
|---|---|
| S1 | Leídos, guardados y feeds coinciden en el móvil y en el PC |
| S2 | El menú lateral muestra «Sincronizado (N min)» |
| S3 | Ajustes: dirección del servicio, clave y botón «Probar»; tamaño de letra; dirección de los gestos |
| S4 | El móvil sincroniza en segundo plano |

## Diseño

Las maquetas son la referencia de la interfaz. Cualquier cambio de interfaz pasa por Rubén y se refleja en ellas.

- `docs/diseño/maquetas.html`: Android, once pantallas.
- `docs/diseño/maquetas-pc.html`: PC, cinco pantallas, en tres columnas.
- `assets/icono/`: el icono, en versión oscura, clara y silueta.

Estilo: minimalista en negro, blanco y gris; letra Roboto; tema claro u oscuro según el sistema.

## Extras para después

Cada uno se analiza aparte cuando lo principal funcione:

1. Traducir el paper completo, no solo el resumen.
2. Traducción local en las apps, como respaldo.
3. Traducción del chino al español.
