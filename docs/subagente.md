# Instrucciones para un subagente de Atalaya

Te lanzó el orquestador de una sesión para hacer **una parte** de una issue de GitHub en tu propio worktree. Trabajas en segundo plano: no puedes hablar con Rubén. Todo lo que sabes llega por estos archivos y por el prompt del orquestador; lo que averigües vuelve en tu informe.

## Pasos

1. **Comprueba dónde estás.** Anota tu rama (`git branch --show-current`) y el commit del que partes (`git log -1 --oneline`). Hecho cuando tienes los dos para el informe.
2. **Lee tu tarea.** Lee la issue con `gh issue view N --comments` y localiza la parte que te dio el orquestador. Lee lo que cita: sus requisitos de `docs/PRD.md` (códigos como [L4]), sus ADR de `docs/adr/` y sus pantallas de `docs/diseño/`. Las convenciones de código e idioma están en `AGENTS.md`. Hecho cuando cada referencia de tu tarea está leída.
3. **Haz tu tarea** dentro de los límites de abajo. Comprueba tú lo que puedas de lo que pide la issue: que compila, que pasan las pruebas y, si hace falta, peticiones a las APIs de `ai-lab`. Lo que necesite el móvil o cambiar `ai-lab` déjalo para el orquestador y apúntalo en «Falta comprobar». Hecho cuando todo lo que puedes comprobar está comprobado, con su comando y su resultado.
4. **Haz commit** en tu rama, con el número de la issue: `#21: repite cada hora la revisión de pendientes`. Puedes hacer varios. Hecho cuando `git status` está limpio.
5. **Devuelve el informe** con la plantilla de abajo. Hecho cuando cada apartado tiene contenido o dice «nada».

## Límites

- **Trabajas solo en el repo.** Las APIs de `ai-lab` (Miniflux, llama-swap, el servicio) se usan por su dominio para probar. Su configuración no se toca: ni archivos de `~/servicios` ni contenedores. Desplegar es cosa del orquestador.
- **Compilas en local, en `ai-lab`, de uno en uno.** Android con `android/compilar.sh` (ver `android/README.md`); Rust con `. /data/bots/env.sh` antes de `cargo`. Otras tareas pueden compilar a la vez y la memoria es poca: envuelve cada compilación o prueba en `flock /tmp/atalaya-compilar.lock`, por ejemplo `flock /tmp/atalaya-compilar.lock android/compilar.sh test` o `flock /tmp/atalaya-compilar.lock cargo test -j 2`. No arranques el emulador.
- **No usas el móvil.** Hay uno solo y lo maneja el orquestador.
- **Los documentos y las issues los cambia el orquestador:** las issues de GitHub, `docs/PRD.md`, `docs/adr/`, `docs/diseño/` y este archivo. Si crees que alguno está mal o falta algo, va en el informe.
- **Si falta un dato que solo sabe Rubén,** para en ese punto, haz commit de lo que funcione y pon la pregunta en el informe. Una suposición tuya se fusionaría como si fuera una decisión.
- **Si hace falta `sudo`,** no lo intentes: pon el comando en el informe.
- **Si ves una mejora fuera de tu tarea,** proponla en el informe; no la hagas.

## Plantilla del informe

```
Issue: #N · Parte: <la que te dio el orquestador>
Rama: <rama> · Parte de: <commit> · Commits: <lista>

Hecho:
<Qué hiciste, en frases cortas.>

Comprobado:
<Cada comando o prueba y su resultado.>

Falta comprobar (orquestador):
<Lo que necesita el móvil o ai-lab, o «nada».>

Preguntas para Rubén:
<Una por línea, con tu recomendación, o «nada».>

Huecos y contradicciones:
<Lo que no cuadra entre issue, PRD, ADR, maquetas y código, o «nada».>
```
