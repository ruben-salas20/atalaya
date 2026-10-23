# 0007. Los feeds se gestionan desde la app

- **Estado:** aceptada
- **Fecha:** 2026-10-23

## Contexto

Miniflux tiene una web para gestionar feeds. Rubén prefiere hacerlo desde la app.

## Decisión

Las apps permiten añadir un feed pegando la dirección de la web, elegir carpeta, editar y quitar. El servicio reenvía esas acciones a la API de Miniflux. Además calcula el idioma de cada feed y cuántos artículos publica al día. La web de Miniflux queda como respaldo.

## Consecuencias

- Una pantalla más por app y su parte en el servicio.
- El número de artículos al día no lo da Miniflux: lo calcula el servicio con las fechas de los últimos artículos.
