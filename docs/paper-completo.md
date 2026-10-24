# Leer el paper completo de arXiv: propuesta

Propuesta de la issue #13, probada el 2026-10-23 con tres papers reales. Está pendiente de que Rubén la apruebe; si la aprueba, se escribe su ADR y se abren las issues de código.

## La propuesta en cinco puntos

1. **Fuente: la versión HTML de arXiv, no el PDF.** El HTML trae párrafos, secciones, tablas e imágenes ya separados, y cada fórmula con su LaTeX (el código con el que el autor la escribió). Del PDF solo sale texto plano con las fórmulas rotas.
2. **Traducción: párrafo a párrafo, como los artículos.** Cada fórmula viaja al modelo como una marca con su LaTeX dentro: `<m id="3">\rightarrow</m>`. El modelo traduce el texto de alrededor y deja la marca igual. No se traducen las fórmulas, la bibliografía ni el contenido de las tablas.
3. **Fórmulas: el servicio las convierte en imágenes SVG** (imágenes vectoriales, nítidas a cualquier tamaño) con MathJax, la biblioteca de fórmulas más usada en la web. Así las dos apps pintan lo mismo y ninguna necesita un motor de fórmulas propio.
4. **Apps: bloques nuevos para fórmulas y tablas.** Android añade la fórmula dentro de la línea de texto y un bloque de tabla. El PC usa los elementos en línea de su biblioteca de texto, que ya pintan SVG; las tablas ya las sabe pintar.
5. **Se lee a petición, no al abrir.** Un paper tarda de 3 a 19 minutos en traducirse. Propongo un botón «Leer completo» que muestre las secciones según llegan.

## Los tres papers de prueba

| Paper | Tipo | Párrafos | Texto a traducir | Fórmulas (en línea + aparte) | Tablas | Imágenes |
|---|---|---|---|---|---|---|
| [2610.31614](https://arxiv.org/abs/2610.31614v1) · The Critical 2D KPZ Process | Matemáticas, 84 páginas | 903 | 139 000 caracteres | 2297 + 477 | 0 | 0 |
| [2610.31646](https://arxiv.org/abs/2610.31646v1) · Low-Bit N-Gram Embeddings | Aprendizaje automático, 24 páginas | 80 | 47 000 caracteres | 327 + 7 | 10 | 4 |
| [2610.31671](https://arxiv.org/abs/2610.31671v1) · Image-to-3D Part Generation | Visión por computador, 22 páginas | 100 | 59 000 caracteres | 130 + 10 | 14 | 9 |

Se eligieron para cubrir los extremos: uno casi todo fórmulas y dos con tablas e imágenes. Los dos últimos son los mismos del conjunto de prueba de la traducción (`docs/traduccion.md`). El texto a traducir no cuenta la bibliografía ni las fórmulas. Las tablas se cuentan por su bloque `ltx_table`; `medir_html.py` cuenta también las anidadas y da 55 en el segundo paper.

## 1. HTML frente a PDF

Los tres papers tienen versión HTML en `https://arxiv.org/html/<id>`. También la tiene «Attention Is All You Need», de 2017: arXiv ha generado el HTML de papers antiguos.

| | HTML de arXiv | PDF |
|---|---|---|
| Párrafos y secciones | Separados: `<p class="ltx_p">`, `<section>` | Hay que adivinarlos. Salen palabras partidas por guiones («equa- tions») y cabeceras de página mezcladas con el texto |
| Fórmulas | Cada una en MathML (el formato estándar de fórmulas en la web) con su LaTeX en el atributo `alttext` | Ilegibles. La ecuación (1.1) del paper KPZ sale como `Bt hpt, xq “ ∆hpt, xq ` 1 |∇hpt, xq|2` |
| Tablas | `<table class="ltx_tabular">`, celda a celda | Texto en columnas, sin estructura |
| Imágenes | Archivos PNG enlazados | Hay que extraerlas del PDF |
| Citas | Enlaces a la bibliografía | Texto plano |

**Conclusión:** el HTML es la única fuente que sirve. Cuando un paper no tenga HTML (por ejemplo, si el autor solo subió el PDF), la app sigue como hoy: resumen traducido y enlace al PDF [P4]. No medí cuántos papers están en ese caso.

**Detalle para el código:** las imágenes usan rutas relativas. Hay que resolverlas contra la dirección con barra final: `https://arxiv.org/html/2610.31646v1/fig_design.png` responde 200; sin la barra, la ruta se duplica y responde 404.

## 2. Traducir sin romper las fórmulas

Probé con Gemma 4 E4B (`gemma-4-e4b-rapido`, el modelo del servicio) sobre 9 párrafos reales, 3 por paper, todos con 2 a 8 fórmulas y con citas. Usé el mismo aviso al modelo que el servicio (`HTML_PROMPT` en `crates/atalaya-server/src/content.rs`).

| Cómo va cada fórmula al modelo | Fórmulas intactas | Citas intactas | Fallos de sentido | Asteriscos añadidos | Tokens de salida |
|---|---|---|---|---|---|
| Tal cual, en MathML | 3 de 3 párrafos | — | — | — | De 2,4 a 6 veces más que con marcas |
| Marca vacía: `<m id="3"></m>` | 9 de 9 | 9 de 9 | 1, repetido en las dos pasadas | 2 de 9 en la primera pasada, 1 de 9 en la segunda | Referencia |
| **Marca con su LaTeX: `<m id="3">\rightarrow</m>`** | **9 de 9, en orden** | 9 de 9 | **0** | **0** | Un 22 % más que la marca vacía |

**El fallo de la marca vacía:** el paper 2610.31646 dice «(1) compress → pre-train». Como la marca no dice que es una flecha, Gemma tradujo «(1) preentrenar [marca] comprimir»: invirtió el orden. Con el LaTeX dentro tradujo bien: «(1) comprimir → preentrenar».

**Los asteriscos:** con la marca vacía, Gemma escribió `*embeddings*` en Markdown (un formato de texto que usa asteriscos para la cursiva) dentro del HTML. La app los mostraría tal cual. Con el LaTeX dentro no pasó, pero la comprobación del servicio (`check`) debería rechazarlos igualmente.

**Recomendación:** marca con LaTeX. Al recomponer, el servicio cambia cada marca por su SVG y comprueba que el LaTeX volvió intacto. Si no, repite el párrafo una vez y, si sigue mal, deja el original, como ya hace con los artículos (ADR 0009).

### Cuánto tarda

En la prueba, Gemma generó unos 94 tokens por segundo. Con esa velocidad, un paper entero tardaría:

| Paper | Tokens de salida, aproximados | Tiempo, aproximado |
|---|---|---|
| 2610.31646 (aprendizaje automático) | 19 000 | 3,4 min |
| 2610.31671 (visión) | 25 000 | 4,3 min |
| 2610.31614 (matemáticas) | 108 000 | 19 min |

Es una estimación a partir de 9 párrafos, no una medida del paper entero. El paper de matemáticas tarda tanto porque su LaTeX va dentro de las marcas: 158 000 caracteres de fórmulas.

Mientras traduce un paper, la GPU no traduce títulos nuevos: comparten cola. Por eso propongo traducir a petición y por secciones, para que Rubén empiece a leer al minuto.

## 3. Pintar las fórmulas

### Qué sabe pintar cada app hoy

| | Android (Compose, `ArticleHtml.kt`) | PC (GPUI, `TextView::html` de gpui-component 0.8.0) |
|---|---|---|
| Fórmulas (`<math>`) | No | No |
| Subíndices y superíndices (`<sub>`, `<sup>`) | No | No |
| Tablas | No | Sí |
| Imágenes | Sí, con Coil (la biblioteca de imágenes de la app) | Sí |
| Imagen SVG dentro de una línea de texto | Posible con `InlineTextContent` de Compose | Posible con `InlineElement` de gpui-base, que alinea la imagen con la línea base del texto. gpui-base trae una prueba que pinta así una fórmula en SVG (`inline_virtual_tests.rs`) |

### Opciones

| Opción | A favor | En contra |
|---|---|---|
| **A. El servicio convierte cada fórmula en SVG con MathJax** (recomendada) | Las dos apps pintan igual; las dos ya muestran imágenes. Se hace una vez por paper y se guarda | El servicio necesita Node.js (el entorno que ejecuta JavaScript fuera del navegador) para MathJax |
| B. Android muestra el paper en un WebView (un navegador dentro de la app) con MathML | Chrome ya pinta MathML | No sirve en el PC: GPUI no tiene navegador. Rompe el tema y el tamaño de letra de la app |
| C. Pasar las fórmulas a texto Unicode (β, ξ, x²) | Sin imágenes | No sirve para fracciones, raíces ni matrices |

### Prueba de la opción A

Convertí con MathJax 3.2.1 las 3206 fórmulas de los tres papers:

- **3202 bien (99,9 %).** Las 4 que fallan, todas del paper KPZ, usan macros internas de LaTeXML (el programa con el que arXiv genera el HTML), como `\lx@scalerel@obj`. Para ellas la app puede mostrar el LaTeX como texto.
- **Rápido:** 4 segundos para las 2731 del paper KPZ; 0,1 s para cada uno de los otros dos.
- **Peso:** 20 MB de SVG en el paper KPZ, 2,2 MB comprimidos con gzip. Unos 0,5 MB sin comprimir en cada uno de los otros dos.
- **Cada SVG trae su tamaño y su alineación** en unidades de la letra (`width="5.429ex"`, `vertical-align: -0.439ex`). Así cada app ajusta la fórmula al tamaño de letra que elija Rubén.
- **Color:** MathJax pinta con `currentColor`. Cada app debe cambiarlo por el color del texto de su tema antes de pintar, o en el tema oscuro las fórmulas saldrían negras.

La ecuación (1.1) del paper KPZ, convertida así, se ve correcta: ∂ₜh(t,x) = ½Δh(t,x) + ½|∇h(t,x)|² + βξ(t,x).

## 4. Tablas e imágenes

- **Tablas:** se muestran sin traducir las celdas, que casi siempre son números y nombres de métodos. Se traduce su título (el pie de tabla). Android necesita un bloque nuevo con desplazamiento lateral, porque una tabla ancha no cabe en el móvil. El PC ya pinta `<table>`.
- **Imágenes:** se muestran como en los artículos. Se traduce su pie.

## Qué haría falta construir

Si Rubén aprueba, se abrirían estas issues de código:

1. **Servicio:** descargar el HTML de arXiv, dividirlo en secciones y párrafos, traducir con marcas de LaTeX, convertir fórmulas a SVG y guardar el resultado. Un endpoint nuevo en `atalaya-api` para pedir el paper completo y su progreso.
2. **Android:** fórmulas en línea y aparte, bloque de tabla, SVG con Coil (necesita su extensión `coil-svg`) y el botón «Leer completo».
3. **PC:** fórmulas con `InlineElement` y el botón «Leer completo».

## Preguntas para Rubén

1. **¿Cuándo se traduce el paper completo?** Recomiendo un botón «Leer completo», con progreso por secciones. Traducirlo al abrir, como dice el ADR 0003 para artículos, ocuparía la GPU hasta 19 minutos por paper.
2. **¿Dónde corre MathJax?** Recomiendo un contenedor pequeño de Node.js en `ai-lab`, al lado del servicio, al que este llama por la red interna. La otra opción es meter Node.js en la imagen del servicio. Las dos cambian `docker-compose.yml`.
3. **¿Se traducen las celdas de las tablas?** Recomiendo que no, al menos al principio: solo el pie de tabla.

## Cómo repetir las pruebas

Scripts en `bench/paper-completo/`. Descarga los papers en una carpeta aparte y corre desde ella; `B` es la ruta de los scripts:

```sh
B=<ruta del repo>/bench/paper-completo
(cd $B && npm install)
curl -L -o 2610.31646v1.html https://arxiv.org/html/2610.31646v1
uv run --with beautifulsoup4 --with lxml python $B/medir_html.py *.html          # texto, fórmulas, tablas
uv run --with beautifulsoup4 --with lxml python $B/traducir.py *.html            # marcas vacías; usa llama-swap
uv run --with beautifulsoup4 --with lxml python $B/traducir_latex.py traducciones.json  # marcas con LaTeX
grep -o 'alttext="[^"]*"' 2610.31646v1.html | sed 's/^alttext="//; s/"$//' > 2610.31646v1.latex
node $B/svg.mjs 2610.31646v1.latex                                           # fórmulas a SVG
```

`traducir.py` y `traducir_latex.py` usan la GPU de `ai-lab`: córrelos solo si ninguna otra sesión la está usando.
