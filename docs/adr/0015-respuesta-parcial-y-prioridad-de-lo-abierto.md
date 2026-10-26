# 0015. Mientras traduce, el servicio entrega lo ya traducido y prioriza lo que se está leyendo

- **Estado:** aceptada
- **Fecha:** 2026-10-25

## Contexto

El servicio traduce un artículo párrafo a párrafo, pero solo lo entregaba al terminar: entre 30 s y 1,5 min en un artículo largo y más de 5 min en un paper. El primer párrafo está listo en 0,5 a 2 s. Además, la cola de la GPU atendía los artículos por orden de llegada: uno abierto después esperaba a que terminara entero el anterior. Medidas y opciones en `docs/leer-por-partes.md` (#32).

Se valoraron:

- **Respuesta parcial** a la misma pregunta de cada 3 s.
- **Eventos del servidor (SSE):** como mucho 3 s antes, pero con un cliente nuevo en cada app.
- **Cambiar el orden de la cola.** Es compatible con cualquiera de las dos.

## Decisión

- Con `translation: Pending`, `GET /api/entries/{id}` trae en `translated_content` el HTML traducido hasta donde se llegó y el resto en inglés. Cada párrafo pendiente va en `<span class="atalaya-pendiente">`. Trae también `progress: {done, total}`. Los dos campos son opcionales: las apps viejas siguen funcionando.
- Los párrafos ya traducidos de un artículo en curso viven en la memoria del servicio. La base solo guarda la traducción completa.
- Cola de la GPU:
  1. títulos nuevos;
  2. párrafos del artículo pedido más recientemente, que cada pregunta de la app renueva;
  3. los demás artículos y papers en curso;
  4. títulos atrasados.

  Dentro de un artículo, siempre de arriba abajo.
- Las apps muestran la versión parcial en «Español», con una barra «Traduciendo… N de M» y los párrafos pendientes en gris, sin mover la posición de lectura. Solo guardan en disco la traducción completa.
- No se usa SSE por ahora.

## Consecuencias

- El primer párrafo en español se ve en 1 a 5 s, y un paper empieza a leerse enseguida.
- Cambia el contrato de forma compatible: primero va el servicio y después las apps.
- Si el servicio se reinicia a medias, el artículo vuelve a empezar.
- El paper completo (#38) usa la misma respuesta parcial.
