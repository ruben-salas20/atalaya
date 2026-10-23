# 0009. Gemma 4 E4B traduce, en su versión `gemma-4-e4b-rapido`

- **Estado:** aceptada
- **Fecha:** 2026-10-23

## Contexto

Atalaya necesita un modelo que traduzca del inglés al español en la RTX 3050 de 6 GB de `ai-lab`, servido por llama-swap (ADR 0004). Los títulos se traducen al llegar y el texto completo al abrirlo (ADR 0003).

El plan preveía medir tres modelos: Gemma 4 E4B, Hunyuan-MT 7B y NLLB-200. Rubén decidió medir solo Gemma, que ya estaba instalado, y probar otro únicamente si su calidad resultaba muy mala. También se descartaron sin medir:

- **LFM2.5-2.6B:** más rápido (unos 100 tok/s), pero en la prueba del paso 1.1 coló marcas como `<translation>` y dejó párrafos sin traducir.
- **NLLB-200 y Opus-MT:** traducen frase a frase, no conservan el HTML y necesitan otro motor.
- **Hunyuan-MT y Hy-MT2:** Hunyuan-MT 7B es lento para 6 GB; según su página, Hy-MT2-1.8B necesita un cambio de llama.cpp que aún no está publicado (sin comprobar).

Las medidas de Gemma están en `docs/traduccion.md`. En resumen: 4037 MiB de GPU, 29,8 s de carga en frío, 0,4 s por título, 4,2 s por resumen y 27 s por noticia de media (62 s la más larga). Títulos y resúmenes salieron sin fallos. En 6 de 10 noticias hubo algún fallo: un párrafo sin traducir, enlaces o cursivas perdidos y párrafos unidos.

Rubén valoró la calidad como suficiente. Le basta con entender la idea, y si un texto no tiene sentido puede cambiar al original en inglés (requisito L5).

## Decisión

El modelo de traducción es `gemma-4-e4b-rapido` de llama-swap: Gemma 4 E4B en Q4_0, con MTP, sin razonamiento previo, 32k de contexto y `ttl: 300`. No hay que cambiar `llama-swap/config.yaml`.

La traducción es al español de Latinoamérica, que es el de Rubén.

## Consecuencias

- La primera traducción tras 5 minutos sin uso espera unos 30 s a que el modelo cargue.
- Una noticia larga tarda hasta un minuto. El aviso «Traduciendo…» (L6) puede durar bastante.
- El servicio (pasos 2.3 y 2.4) debe:
  - pedir en la instrucción español de Latinoamérica y mayúsculas solo al principio de los títulos, como se escribe en español;
  - comprobar cada párrafo traducido: si vuelve en inglés o le faltan etiquetas `<a>` o `<em>`, reintentarlo, y si sigue mal, dejar el párrafo original.
- Expresiones de moda como «vibe coding» a veces se traducen mal. Se acepta: el original está a un toque.
- TranslateGemma 4B, una Gemma ajustada para traducir, queda como extra para evaluar al final del plan (paso 6.4).
