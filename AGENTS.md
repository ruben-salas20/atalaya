# Atalaya

Lector RSS personal de Rubén que traduce al español en su servidor `ai-lab`. Hay un servicio en Rust, una app de Android y otra de PC. Qué hace: `docs/PRD.md`. Por qué así: `docs/adr/`. Qué toca ahora: las [issues de GitHub](https://github.com/ruben-salas20/atalaya/issues) (`gh issue list`). Cómo se construyó: `docs/plan.md`, un plan ya terminado.

Cada sesión trabaja en **una issue**. Quien abre la sesión es el **orquestador**: trabaja en la rama `issue-N`, reparte los pasos de código entre **subagentes**, cada uno en su worktree (una copia del repo en otra carpeta, con su propia rama), fusiona su trabajo y abre un PR a `main`. Todo lo que necesitas está en archivos y en las issues; nunca en una conversación anterior.

**Si te lanzó un orquestador para hacer un paso, eres subagente:** sigue `docs/subagente.md` y no los pasos de abajo.

## Pasos del orquestador

1. **Elige la issue.** Si Rubén te dice cuál, es esa. Si no, pregúntale con una recomendación. Lee la issue y sus comentarios con `gh issue view N --comments`. Si la rama `issue-N` ya existe en `origin` y puede que otra sesión la esté llevando, pregunta a Rubén antes de seguir. Si existe (local o en `origin`), cámbiate a ella y sigue desde la primera casilla sin marcar. Si no, créala desde `main` al día (`git pull`). Hecho cuando estás en `issue-N` y sabes qué falta.
2. **Divide y reparte.** Si la issue tiene varias partes, escríbelas como casillas `- [ ]` en su descripción, si no lo están ya. Decide quién hace cada una: tú, las que tocan un recurso único (ver la tabla de abajo); un subagente, las de código. Antes de lanzar nada, pregunta a Rubén los datos que falten, de uno en uno y con una recomendación: los subagentes no pueden preguntarle. Hecho cuando cada parte tiene responsable y no falta ningún dato conocido.
3. **Haz tus pasos.** Haz commit de cada uno en `issue-N`. Hecho cuando cada paso tuyo cumple lo que pide la issue, comprobado por ti.
4. **Lanza los subagentes.** Antes, haz commit y anota su hash corto (`git rev-parse --short HEAD`): el worktree nace desde `main`, no desde tu rama, y el subagente debe moverse a ese commit. Lanza cada uno con la herramienta de agentes y aislamiento `worktree`, con este prompt: «Lee `<ruta absoluta del repo>/docs/subagente.md` y síguelo. Tu tarea: issue #N, <la parte concreta y su "hecho cuando">. Debes partir del commit <hash>: si `git log -1 --oneline` muestra otro y tu rama no tiene commits propios, haz `git reset --hard <hash>`.», más los datos que Rubén dio en esta sesión. Lanza a la vez solo partes que no toquen los mismos archivos. Hecho cuando cada subagente ha devuelto su informe.
5. **Comprueba y fusiona.** Por cada informe: fusiona su rama en `issue-N` con `git merge --no-ff`, resuelve los conflictos, y repite tú las pruebas que cita y las de «Falta comprobar». Un subagente informa igual de seguro cuando acierta que cuando se equivoca: la casilla de la issue se marca solo con tu prueba. Lleva a Rubén sus preguntas. Hecho cuando cada parte está marcada y comprobada por ti.
6. **Cierra.** Sube `issue-N` y abre el PR con la plantilla de abajo; la línea `Closes #N` cierra la issue al fusionar. Cada cosa nueva que quede pendiente, idea o duda se abre como issue propia, con su origen. Fusiónalo tú cuando cada parte esté comprobada por ti; si cambia algo que se nota en las apps, publica después una versión (ADR 0012). Pregunta antes a Rubén solo si el PR trae una decisión importante o difícil que él no haya tomado. Tras la fusión, borra `issue-N` en local y en `origin`, y los worktrees de sus subagentes. Dile a Rubén qué se hizo y que abra una sesión nueva. Hecho cuando el PR está fusionado, o abierto y con su enlace dado a Rubén si espera su decisión.

Si la sesión se va a cortar antes, deja un comentario en la issue que empiece por «A medias:» y diga qué falta, haz commit y sube `issue-N`: la siguiente sesión sigue desde esa rama.

Un subagente cuesta tokens y una fusión. Para un paso pequeño o que depende del anterior, hazlo tú.

### Issues de investigación

Las issues con la etiqueta `investigación` terminan en una propuesta para Rubén, no en código. La propuesta va en un comentario de la issue, con sus pruebas; si es larga, en un documento de `docs/`. Si Rubén la aprueba, se escribe su ADR y se abren las issues de código.

### Dos issues a la vez

Cada issue va en su sesión y su rama. Para no chocar:

- **Servicio y contrato:** si tu issue necesita cambiar `crates/atalaya-api` o `crates/atalaya-server` y otra sesión trabaja en una app, haz ese cambio primero, en una rama `issue-N-servicio` y un PR propio a `main`. Lo fusionas tú; después se despliega. La otra sesión lo recoge con `git merge main` en su rama.
- **`ai-lab`, la GPU y el móvil:** solo una sesión a la vez. Si dudas, pregunta a Rubén.

### Qué hace cada uno

| Recurso | Quién lo usa | Por qué |
|---|---|---|
| Configuración de `ai-lab` (Compose, Caddy, DNS, llama-swap) | Solo el orquestador | Vive fuera del repo: los worktrees no la aíslan. Además, cada cambio necesita la aprobación de Rubén |
| GPU (medir o cargar modelos) | Solo el orquestador, de uno en uno | Con 6 GB cabe un modelo: en paralelo se turnan y falsean los tiempos |
| Móvil (`adb`) | Solo el orquestador, de uno en uno | Hay uno solo |
| APIs de `ai-lab` (Miniflux, llama-swap, el servicio) por su dominio | Todos | Leer y probar no cambia la configuración |
| Código del repo | Subagentes, o el orquestador en pasos pequeños | Cada worktree aísla su rama |
| Issues de GitHub (abrir, marcar casillas, comentar) | Solo el orquestador | Los subagentes proponen en su informe; así no se pisan |

### Plantilla del PR

```markdown
Título: <qué se consigue> (#N)

## Qué hace
<Dos o tres frases.>

## Pasos
- <parte> — comprobado con <prueba>
- …

## Cómo probarlo
<Comandos o acciones concretas.>

## Pendiente o dudas
<Lo que quedó fuera o necesita decisión, con el número de la issue que se abrió para cada cosa. «Nada» si no hay.>

Closes #N

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

## Documentos y quién los cambia

| Archivo | Quién lo cambia |
|---|---|
| `docs/PRD.md` | Solo con la aprobación de Rubén |
| `docs/adr/` | Un ADR nuevo cuando Rubén aprueba una decisión que cambia la arquitectura, la tecnología o lo que hace la app. Un ADR aceptado no se reescribe: si cambia, se escribe otro y en el viejo solo cambia la línea de estado a «sustituida por NNNN» |
| `docs/plan.md` | Nadie: es la historia del plan terminado |
| Issues de GitHub | El orquestador las abre con lo pendiente y marca sus casillas. Cambiar lo que pide una issue, solo con Rubén |
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
- **Commits:** uno por paso terminado, con el número de la issue: `#21: repite cada hora la revisión de pendientes`.
- **Ramas:** el orquestador sube solo `issue-N` (y `issue-N-servicio`) y las etiquetas de versión `vX.Y.Z`. Nunca hace `push` a `main`: los cambios entran por PR.
- **Repo público** (ADR 0012): todo lo que se sube se ve, también la historia. Nunca entran secretos, datos personales de Rubén ni textos ajenos completos. Antes de cada `push`, `gitleaks git` sobre la rama.
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
