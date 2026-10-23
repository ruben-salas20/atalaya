# 0004. El modelo de traducción se sirve por llama-swap

- **Estado:** aceptada
- **Fecha:** 2026-10-23

## Contexto

La GPU de `ai-lab` es una RTX 3050 de 6 GB: no caben dos modelos a la vez. llama-swap ya arranca el modelo que se le pide y lo saca de la GPU tras su `ttl` (el tiempo sin uso). Hoy sirve modelos de lenguaje, Whisper y la voz de qwen3-tts.

## Decisión

El modelo de traducción se sirve por llama-swap, aunque no sea un modelo de llama.cpp. En ese caso llama-swap lo arranca como un programa aparte, igual que hace con la voz. El servicio de Atalaya solo le hace peticiones.

## Consecuencias

- El servicio no carga ni descarga modelos.
- No hay choques de memoria con los otros modelos de la torre.
- Si otro modelo está cargado, la primera traducción espera a que llama-swap lo cambie.
