# Leer el artículo mientras se traduce (#32)

Propuesta para Rubén. Si la aprueba, se escribe su ADR y se abren las issues de código.

## Resumen

- **Hoy** el servicio traduce un artículo párrafo a párrafo, pero solo lo entrega al terminar. En un artículo largo eso son entre 30 s y 1,5 min de espera; en un paper, más de 5 min.
- **Propuesta:** opción 1, la respuesta parcial. Mientras traduce, el servicio responde con el HTML ya traducido hasta donde llegó y el resto en inglés, más cuántos párrafos lleva. Las apps siguen preguntando cada 3 s, como hoy.
- **Además, el orden de la cola:** lo que tienes abierto pasa delante. Hoy, si abres un artículo mientras se traduce otro, el segundo espera a que termine entero el primero.
- **Se descarta por ahora** la opción 2, los eventos del servidor (SSE): ganaría como mucho 3 s y obliga a escribir un cliente nuevo en cada app.

## Medidas

`bench/por-partes/medir.py`, el 2026-10-25, con `gemma-4-e4b-rapido` y el código del servicio, párrafo a párrafo y en orden, como hace la cola. Las tres noticias más largas del conjunto de prueba y el paper 2610.31646 (cuerpo sin bibliografía).

| Texto | Párrafos | Primer párrafo | 25 % | 50 % | Total |
|---|---|---|---|---|---|
| noticia-10 (Paul Graham) | 52 | 18,1 s* | 29,9 s | 45,7 s | 1 min 20 s |
| noticia-09 (Ars Technica) | 35 | 1,5 s | 12,5 s | 23,7 s | 43 s |
| noticia-08 (The Verge) | 14 | 0,5 s | 5,1 s | 12,5 s | 33 s |
| Paper 2610.31646 | 429 | 0,5 s | 2 min | 3 min 36 s | 5 min 17 s |

\* Incluye la carga del modelo en la GPU: fue lo primero de la medida. Con el modelo ya cargado, el primer párrafo tarda entre 0,5 y 2 s.

Lo que significa para el lector, con la respuesta parcial:

- **Artículo largo:** el primer párrafo en español se ve en 1 a 5 s, contando los 3 s de la pregunta, en vez de en 30 s a 1,5 min. Se lee más despacio de lo que traduce la GPU (unos 1,5 s por párrafo), así que casi nunca se alcanza el final de lo traducido.
- **Paper:** empieza a leerse en unos 5 s en lugar de 5 min, si el resto de la cola está libre. El paper de 19 min (2610.31614) también empieza a leerse enseguida.

## Cómo sería

### Servicio y contrato (`atalaya-api`)

- `GET /api/entries/{id}` con `translation: Pending` trae, además:
  - `translated_content`: el HTML con los párrafos ya traducidos y el resto en inglés. Cada párrafo que falta va envuelto en `<span class="atalaya-pendiente">…</span>`, para que las apps lo marquen.
  - `progress: { done, total }`: cuántos párrafos van y cuántos hay.
- Los campos son nuevos y opcionales: una app vieja los ignora y sigue como hoy. No obliga a actualizar las dos apps a la vez.
- El servicio guarda en memoria los párrafos ya traducidos de cada artículo en curso. Si se reinicia a medias, el artículo vuelve a empezar, como hoy.
- **Regla del ADR 0009:** no cambia. Un párrafo que sale mal se repite una vez y, si sigue mal, queda el original; en la respuesta parcial, ese párrafo aparece ya como terminado, en inglés.

### Orden de la cola de la GPU

- **Hoy:** títulos nuevos, después párrafos por orden de llegada y al final títulos atrasados.
- **Propuesta:**
  1. Títulos nuevos: cada uno tarda alrededor de 1 s, así que no se nota.
  2. Párrafos del artículo que se pidió más recientemente. Cada pregunta de la app (cada 3 s) lo renueva: el que estás leyendo va delante.
  3. Los demás artículos y papers en curso, por orden.
  4. Títulos atrasados.
- **Dentro de un artículo,** los párrafos siempre en orden, de arriba abajo.
- **Paper:** igual que un artículo. Si lo dejas para leer otra cosa, sigue traduciéndose detrás. Las secciones (`<section>`) del HTML de arXiv son párrafos agrupados: como van en orden, cada sección aparece completa en cuanto termina su último párrafo. Las fórmulas se convierten antes de traducir (#38), así que un párrafo nunca enseña marcas `<m>` sueltas.

### Lector (apps)

- **Selector «Español / Original»:** queda igual. «Español» muestra la versión parcial.
- **Barra fina de progreso** bajo la cabecera: «Traduciendo… 12 de 52». Desaparece al terminar.
- **Párrafos que faltan:** en inglés, en gris claro. Cuando llegan, cambian a español sin mover la lectura: no se salta a otra parte.
- **Al terminar:** queda como hoy con `Done`.

Boceto, en la app de PC:

```
┌───────────────────────────────────────────────────────────────┐
│ [Español | Original]                         ◉  🔖  ↗  ⋮       │
│ ▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░░░░░  Traduciendo… 12 de 52         │
├───────────────────────────────────────────────────────────────┤
│ Paul Graham · hace 2 días                                       │
│ Rendimientos superlineales                                      │
│                                                                 │
│ Una de las cosas más importantes que no entendí sobre el        │
│ mundo cuando era niño es el grado en que los rendimientos…      │
│                                                                 │
│ (…párrafos 2 a 12 en español…)                                  │
│                                                                 │
│ There are two fundamental causes of superlinear returns…        │  ← gris claro
│ (…el resto, en inglés y en gris, hasta que llega…)              │
└───────────────────────────────────────────────────────────────┘
```

### Sin red y caché

- **Copia del móvil y del PC:** solo guarda la traducción cuando está completa (`Done`), como hoy.
- **Si cierras el artículo a medias:** el servicio sigue traduciéndolo detrás. Al volver, ves hasta donde llegó o ya entero.
- **Sin red:** se ve lo último que la app tenía en disco. Una versión parcial no se guarda: sería español a medias para siempre si no vuelve la red.

## Issues de código, si se aprueba

1. **Servicio:** respuesta parcial con `progress`, y nueva prioridad de la cola. Es un cambio de contrato, en `issue-N-servicio`, pero compatible.
2. **App de PC:** barra de progreso, párrafos pendientes en gris y conservar la posición de lectura al refrescar.
3. **App de Android:** lo mismo.

Las tres del paper completo (#38, #39 y #40) se apoyan en estas: la #38 usa la misma respuesta parcial.

## Lo que no se hace

- **SSE:** se puede añadir después sin romper nada, si los 3 s de la pregunta se notan.
- **Traducir por adelantado al abrir la lista:** gastaría GPU en artículos que no se leen (ADR 0003).
