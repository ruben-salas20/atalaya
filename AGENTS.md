# Atalaya

Lector RSS personal de Rubén que traduce al español en su servidor `ai-lab`. Hay un servicio en Rust, una app de Android y otra de PC. Qué hace: `docs/PRD.md`. Por qué así: `docs/adr/`. Qué toca ahora: `docs/plan.md`.

Cada sesión hace **una fase** del plan. Quien abre la sesión es el **orquestador**: trabaja en la rama `fase-N`, reparte los pasos de código entre **subagentes**, cada uno en su worktree (una copia del repo en otra carpeta, con su propia rama), fusiona su trabajo y abre un PR a `main`. Todo lo que necesitas está en archivos; nunca en una conversación anterior.

**Si te lanzó un orquestador para hacer un paso, eres subagente:** sigue `docs/subagente.md` y no los pasos de abajo.

## Pasos del orquestador

1. **Encuentra la fase en curso.** Es la primera fase de `docs/plan.md` con casillas `- [ ]`. Lee la primera línea de la Bitácora, al final del plan. Si existe la rama `fase-N` (local o en `origin`), cámbiate a ella y sigue desde la primera casilla sin marcar. Si no, créala desde `main` al día (`git pull`). Hecho cuando estás en `fase-N` y sabes qué pasos faltan.
2. **Reparte.** Cada fase tiene una línea «Reparto» que dice qué pasos son del orquestador y cuáles pueden ir en paralelo. Antes de lanzar nada, pregunta a Rubén los datos que falten para toda la fase, de uno en uno y con una recomendación: los subagentes no pueden preguntarle. Hecho cuando cada paso pendiente tiene responsable y no falta ningún dato conocido.
3. **Haz tus pasos.** Son los que tocan un recurso único: `ai-lab`, la GPU o el móvil (ver la tabla de abajo). Haz commit de cada uno en `fase-N` con su número. Hecho cuando cada paso tuyo cumple su «Hecho cuando», comprobado por ti.
4. **Lanza los subagentes.** Antes, haz commit: el worktree parte del último commit. Lanza cada uno con la herramienta de agentes y aislamiento `worktree`, con este prompt: «Lee `<ruta absoluta del repo>/docs/subagente.md` y síguelo. Tu paso: N.M.», más los datos que Rubén dio en esta sesión. Lanza a la vez solo los pasos del mismo grupo del Reparto; el resto, de uno en uno. Hecho cuando cada subagente ha devuelto su informe.
5. **Comprueba y fusiona.** Por cada informe: fusiona su rama en `fase-N` con `git merge --no-ff`, resuelve los conflictos, y repite tú las pruebas que cita y las de «Falta comprobar». Un subagente informa igual de seguro cuando acierta que cuando se equivoca: la casilla `[x]` se marca solo con tu prueba. Lleva a Rubén sus preguntas. Hecho cuando cada paso de la fase está marcado y comprobado por ti.
6. **Cierra la fase.** Añade la línea de la Bitácora, haz commit, sube `fase-N` y abre el PR con la plantilla de abajo. Dile a Rubén qué se hizo, que revise y fusione el PR, y que después abra una sesión nueva. El PR lo fusiona Rubén, nunca tú. Hecho cuando el PR está abierto y le has dado su enlace.

Si la sesión se va a cortar antes, apunta «a medias» en la Bitácora, haz commit y sube `fase-N`: la siguiente sesión sigue desde esa rama.

Un subagente cuesta tokens y una fusión. Para un paso pequeño o que depende del anterior, hazlo tú.

### Qué hace cada uno

| Recurso | Quién lo usa | Por qué |
|---|---|---|
| Configuración de `ai-lab` (Compose, Caddy, DNS, llama-swap) | Solo el orquestador | Vive fuera del repo: los worktrees no la aíslan. Además, cada cambio necesita la aprobación de Rubén |
| GPU (medir o cargar modelos) | Solo el orquestador, de uno en uno | Con 6 GB cabe un modelo: en paralelo se turnan y falsean los tiempos |
| Móvil (`adb`) | Solo el orquestador, de uno en uno | Hay uno solo |
| APIs de `ai-lab` (Miniflux, llama-swap, el servicio) por su dominio | Todos | Leer y probar no cambia la configuración |
| Código del repo | Subagentes, o el orquestador en pasos pequeños | Cada worktree aísla su rama |
| `docs/plan.md` | Solo el orquestador | Si lo tocan varios, cada fusión da conflicto |

### Plantilla del PR

```markdown
Título: Fase N: <qué se consigue>

## Qué hace
<Dos o tres frases.>

## Pasos
- N.1 <qué se hizo> — comprobado con <prueba>
- N.2 …

## Cómo probarlo
<Comandos o acciones concretas.>

## Pendiente o dudas
<Lo que quedó fuera o necesita decisión. «Nada» si no hay.>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

## Bitácora

Una línea por sesión, con este formato y 25 palabras como mucho tras el estado:

```
- AAAA-MM-DD · fase N · hecha | a medias · <qué se hizo o qué falta>. Siguiente: <fase o paso>.
```

La más reciente va arriba. Se guardan las 10 últimas: borra la más antigua al añadir la undécima. El detalle vive en los commits y en los PR.

## Documentos y quién los cambia

| Archivo | Quién lo cambia |
|---|---|
| `docs/PRD.md` | Solo con la aprobación de Rubén |
| `docs/adr/` | Un ADR nuevo cuando Rubén aprueba una decisión que cambia la arquitectura, la tecnología o lo que hace la app. Un ADR aceptado no se reescribe: si cambia, se escribe otro y en el viejo solo cambia la línea de estado a «sustituida por NNNN» |
| `docs/plan.md` | El orquestador marca casillas y escribe la Bitácora. Añadir, quitar o reordenar pasos, solo con Rubén |
| `docs/subagente.md` | Solo con Rubén |
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
- **Commits:** uno por paso terminado, con el número del paso: `2.3: traduce los títulos al llegar`.
- **Ramas:** el orquestador sube solo `fase-N`. Nunca hace `push` a `main` ni fusiona el PR.
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
- **Ver HTML local:** el MCP de Playwright bloquea `file://`. Sirve la carpeta con `python3 -m http.server --bind 127.0.0.1` y ábrela por `http://127.0.0.1`. Obscura, su navegador, tarda unos 19 s en pintar las maquetas enteras y el MCP corta a los 5: captura un elemento suelto. Además dibuja mal las capas superpuestas: usa las capturas para la estructura, no para juzgar el aspecto. Para un SVG, `rsvg-convert` y lee la imagen.
