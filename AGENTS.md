# Atalaya

Lector RSS personal de Rubén que traduce al español en su servidor `ai-lab`. Hay un servicio en Rust, una app de Android y otra de PC. Qué hace: `docs/PRD.md`. Por qué así: `docs/adr/`. Qué toca ahora: `docs/plan.md`.

Cada sesión hace **un paso** del plan y lo deja cerrado. Todo lo que necesitas está en archivos; nunca en una conversación anterior.

## Pasos de cada sesión

1. **Encuentra el paso en curso.** Es la primera casilla `- [ ]` de `docs/plan.md`. Lee también la primera línea de la Bitácora, al final del plan: si dice «a medias», sigue desde ahí. Hecho cuando sabes qué paso toca y en qué punto quedó.
2. **Lee solo lo que el paso cita:** sus requisitos del PRD (los códigos como [L4]), sus ADR y las pantallas de las maquetas. Hecho cuando cada referencia del paso está leída.
3. **Haz el paso** hasta cumplir su «Hecho cuando», con prueba real: una petición con `curl`, la app en el móvil, una captura. Si falta un dato, pregunta a Rubén, una pregunta por vez y con una recomendación. Si ves una mejora fuera del paso, proponla al final; no la hagas. Hecho cuando la condición del paso se cumple y la has comprobado.
4. **Cierra el paso.** Marca su casilla `[x]`, añade una línea arriba de la Bitácora y haz commit. Hecho cuando `git status` está limpio.
5. **Despídete.** Dile a Rubén qué se hizo, qué paso sigue, y que abra una sesión nueva para él. Termina aquí, aunque quede contexto libre.

Si la sesión se va a cortar antes de terminar, apunta en la Bitácora «a medias» y qué falta, y haz commit de lo que funcione.

## Bitácora

Una línea por sesión, con este formato y 25 palabras como mucho tras el estado:

```
- AAAA-MM-DD · <paso> · hecho | a medias · <qué se hizo o qué falta>. Siguiente: <paso>.
```

La más reciente va arriba. Se guardan las 10 últimas: borra la más antigua al añadir la undécima. El detalle vive en los commits.

## Documentos y quién los cambia

| Archivo | Quién lo cambia |
|---|---|
| `docs/PRD.md` | Solo con la aprobación de Rubén |
| `docs/adr/` | Un ADR nuevo cuando Rubén aprueba una decisión que cambia la arquitectura, la tecnología o lo que hace la app. Un ADR aceptado no se reescribe: si cambia, se escribe otro y en el viejo solo cambia la línea de estado a «sustituida por NNNN» |
| `docs/plan.md` | El agente marca casillas y escribe la Bitácora. Añadir, quitar o reordenar pasos, solo con Rubén |
| `docs/diseño/` | Las maquetas son la referencia de la interfaz. Se cambian solo con Rubén |

Plantilla de un ADR nuevo, en `docs/adr/NNNN-titulo-en-minusculas.md`, con el número siguiente al último:

```markdown
# NNNN. <Decisión, en una frase>

- **Estado:** aceptada
- **Fecha:** AAAA-MM-DD

## Contexto
<Qué problema había y qué opciones se valoraron.>

## Decisión
<Qué se eligió.>

## Consecuencias
<Qué se gana, qué se pierde y qué obliga a hacer.>
```

## Convenciones

- **Idioma:** nombres del código en inglés. Comentarios, documentos, mensajes a Rubén y commits en español claro, según la norma UNE-ISO 24495-1: lo importante primero, frases cortas y cada término técnico explicado la primera vez.
- **Commits:** uno por paso terminado, con el número del paso: `0.2: instala Miniflux en ai-lab`. Nunca `push` sin que Rubén lo pida.
- **`sudo`:** pide contraseña y no puedes escribirla. Dale a Rubén el comando para su terminal y comprueba después que quedó.

## ai-lab

La torre de Rubén, con GPU RTX 3050 de 6 GB. Se entra con `ssh ai-lab`.

- **Servicios:** en `~/servicios/`, con Docker Compose (proyecto `ai-lab`) y Caddy como única entrada. Lee `~/docs/ai-lab/ai-lab-stack.md` antes de tocar nada: tiene los puertos libres, cómo se añaden los dominios y las convenciones. Actualízalo con cada cambio.
- **Enseña y espera.** Antes de cambiar `docker-compose.yml`, el `Caddyfile`, `llama-swap/config.yaml` o el DNS, enseña a Rubén el cambio exacto y espera su aprobación. Al aplicarlo, comprueba que los servicios que ya había siguen en marcha. Ahí viven sus contraseñas (Vaultwarden) y su DNS (AdGuard).
- **Secretos:** van en `~/servicios/.env`. Nunca muestres sus valores; para comprobar, enseña solo los nombres.
- **GPU:** todo modelo pasa por llama-swap (ADR 0004). Un proceso que use la GPU por su cuenta choca con los modelos que llama-swap carga. Referencia: `~/docs/ai-lab/llama-swap-referencia.md`.
- **Dominios:** `*.rubensalas.dev`, solo dentro de Tailscale (ADR 0008). Nunca por Cloudflare Tunnel.

## Portátil y móvil

- **Móvil:** Samsung A35 (SM-A356E) con Android 16, conectado por USB. Instala y prueba con `adb`; para ver la pantalla, `adb exec-out screencap -p > captura.png` y lee la imagen.
- **Ver HTML local:** el navegador de Playwright (Obscura) bloquea `file://` y `127.0.0.1`. Para comprobar un SVG, conviértelo con `rsvg-convert` y lee la imagen.
