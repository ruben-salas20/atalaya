# 0013. El servicio se despliega desde GHCR con un script de permisos acotados

- **Estado:** aceptada
- **Fecha:** 2026-10-25

## Contexto

El agente que mantiene Atalaya trabaja en `ai-lab` como el usuario `openbot`. No tiene Docker ni acceso a `~/servicios`, así que cada despliegue del servicio esperaba a Rubén. Compose construía la imagen desde una copia del repo en `~/servicios/atalaya`.

Se valoraron tres opciones:

- Dar a `openbot` el grupo `docker`. Equivale a root sobre todo `ai-lab`: `.env`, Vaultwarden y AdGuard incluidos.
- Dar a `openbot` acceso a `/home/ruben`. No basta para reiniciar contenedores, y expone las claves SSH, el token de GitHub y `.env`.
- Un script de root que solo sabe desplegar, ver los registros y el estado del contenedor `atalaya`, con una regla de sudo solo para él.

El agente lee contenido de fuera (artículos, webs, issues) que podría intentar engañarlo. Con la tercera opción, lo peor que puede hacer un engaño es desplegar otra versión de Atalaya.

## Decisión

- **Imagen en GHCR.** El flujo «Publicar versión» construye la imagen del servicio y la sube a `ghcr.io/ruben-salas20/atalaya-server` (GHCR es el registro de imágenes de GitHub): `main` y `sha-<commit>` en cada cambio de `main`; `vX.Y.Z` y `latest` en cada versión.
- **Script `atalaya-servicio`** (`scripts/ai-lab/`), instalado por Rubén como root en `/usr/local/sbin/`, con la regla `scripts/ai-lab/sudoers`. Órdenes: `desplegar <etiqueta>`, `logs [líneas]` y `estado`. Valida cada argumento y no muestra la configuración ni las variables.
- **Etiqueta local fija.** `desplegar` descarga la imagen y le pone la etiqueta `atalaya-server:desplegada`, que es la que usa Compose, con `pull_policy: never`. Un `docker compose up -d` cualquiera no cambia la versión, y volver atrás es desplegar la etiqueta anterior.
- **Lo demás sigue igual.** Los cambios en Compose, Caddy, DNS, llama-swap o `.env` los aplica Rubén tras ver el cambio exacto.

## Consecuencias

- El agente despliega y revisa el servicio sin esperar a Rubén, y sin poder leer secretos ni tocar otros servicios.
- Compose ya no compila: `~/servicios/atalaya` sobra.
- Un cambio en el script exige que Rubén lo vuelva a instalar: el agente no puede modificar el que está instalado.
- Si GHCR no responde, no se puede desplegar una versión nueva. La que está en marcha sigue funcionando.
