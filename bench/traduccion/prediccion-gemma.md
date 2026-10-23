# Predicción: gemma-4-e4b-rapido (paso 1.2)

Escrita el 2026-10-23, antes de medir. Los resultados van en `docs/traduccion.md`, comparados con esta predicción.

## Configuración

`gemma-4-e4b-rapido` de llama-swap: Gemma 4 E4B en Q4_0, con MTP (`--spec-draft-n-max 4`), `--reasoning off`, 32k de contexto y caché KV en q8_0. Plantilla `gemma` de `plantillas.json`, temperatura 0,2. Noticias párrafo a párrafo.

## Plan de medida

- Tres pasadas completas de los 40 textos. La primera con `--descargar-antes`, para medir la carga en frío; las otras dos con el modelo ya cargado.
- Memoria de GPU: `nvidia-smi` cada 2 s durante las tres pasadas; se apunta el máximo.

## Predicción

| Medida | Predicción | Por qué |
|---|---|---|
| Memoria de GPU | 3,7 a 3,9 GB | Medido 3775 MiB con esta configuración el 2026-09-08 (`llama-swap-referencia.md`) |
| Carga en frío (primera petición) | 10 a 40 s | 4,3 GB desde disco mecánico; menos si siguen en la caché de páginas |
| Título | 0,3 a 0,8 s | 10-20 tokens a ~60 tok/s, más el procesado del prompt |
| Resumen de arXiv | 4 a 8 s | 250-450 tokens a ~60 tok/s |
| Noticia | media 30 a 50 s | LFM2.5 tardó 27,5 s de media a ~100 tok/s; Gemma va a ~60 |
| Variación entre pasadas templadas | menos del 10 % | Temperatura baja y nada más en la GPU |
| Calidad | HTML conservado en casi todos los párrafos; sin marcas coladas | Gemma sigue bien las instrucciones; LFM2.5 coló 9 marcas |
