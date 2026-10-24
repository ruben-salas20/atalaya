# 0010. El paper completo se lee a petición, desde el HTML de arXiv, con las fórmulas en SVG

- **Estado:** aceptada
- **Fecha:** 2026-10-23

## Contexto

Atalaya traduce el título y el resumen de cada paper (ADR 0005). Rubén quiere leer también el paper entero. La investigación de la issue #13 (`docs/paper-completo.md`) lo probó con tres papers reales y comparó:

- **Fuente:** el PDF o la versión HTML que arXiv genera con LaTeXML (el programa que convierte LaTeX en HTML). Del PDF las fórmulas salen ilegibles; el HTML separa párrafos, tablas e imágenes y guarda el LaTeX de cada fórmula.
- **Cómo pasar las fórmulas al modelo:** tal cual en MathML, como marca vacía o como marca con su LaTeX dentro. La marca vacía invirtió el sentido de una frase y dejó pasar Markdown. La marca con LaTeX no falló en 9 párrafos.
- **Cómo pintarlas:** convertirlas en SVG en el servicio, mostrarlas en un WebView (un navegador dentro de la app) o pasarlas a texto Unicode. El WebView no existe en GPUI y el Unicode no sirve para fracciones ni matrices.
- **Dónde corre MathJax:** en un contenedor aparte o dentro del servicio.

Un paper entero tarda de unos 3 a 19 minutos en traducirse, según cuántas fórmulas tenga.

## Decisión

- **Fuente:** el HTML de `https://arxiv.org/html/<id>/`. Si un paper no lo tiene, la app sigue como hoy: resumen traducido y enlace al PDF.
- **A petición:** un botón «Leer completo» en el paper abierto. Abrir un paper sigue traduciendo solo el resumen. La traducción se muestra según llega, con lo que decida la issue #32.
- **Traducción:** párrafo a párrafo, como los artículos. Cada fórmula viaja como `<m id="N">LaTeX</m>`; el servicio comprueba que el LaTeX vuelve intacto y sin Markdown. Se traducen los pies de figura y de tabla, y las celdas de tabla que tienen palabras, mandando la tabla entera de una vez. Los números, las fórmulas y la bibliografía no se traducen.
- **Fórmulas:** el servicio las convierte en SVG con MathJax y las guarda con la traducción. Las que MathJax no entiende se muestran como su LaTeX en texto.
- **MathJax dentro del servicio:** la imagen del servicio instala Node.js y el servicio lo lanza una vez por paper, con todas las fórmulas juntas. No hay contenedor nuevo ni cambio en `docker-compose.yml`.
- **Apps:** pintan las fórmulas como imágenes SVG dentro de la línea de texto, con el color del texto de su tema, y muestran las tablas.

## Consecuencias

- La GPU trabaja varios minutos por paper. Los títulos de feeds nuevos esperan mientras tanto; la issue #32 decide el orden de la cola.
- La imagen del servicio crece unas decenas de MB por Node.js y MathJax. No está medido.
- El paper completo no sigue la regla del ADR 0003 de traducir el texto completo al abrir. Para los artículos, esa regla no cambia.
- El servicio necesita un endpoint nuevo en `atalaya-api`, y las dos apps, un bloque de tabla (solo Android) y fórmulas en línea.
- Las imágenes del HTML usan rutas relativas: hay que resolverlas contra la dirección con barra final.
