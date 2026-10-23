# Plan de Atalaya

El plan va en fases, y cada fase en pasos. **Cada fase es una sesión**: un orquestador en la rama `fase-N` reparte los pasos entre subagentes y termina con un PR a `main`. Cada paso termina en algo que se comprueba. La fase en curso es la primera con casillas sin marcar; las fases 3 y 5a pueden ir a la vez, cada una en su sesión. Cómo se trabaja está en `AGENTS.md`.

La línea **Reparto** de cada fase dice qué pasos hace el orquestador y cuáles pueden ir en paralelo. Los pasos entre corchetes del mismo grupo van a la vez; los grupos, uno detrás de otro.

Los códigos entre corchetes, como [L4], son requisitos de `docs/PRD.md`.

| Fase | Qué se consigue |
|---|---|
| 0. Preparación | Monorepo creado y Miniflux funcionando en `ai-lab` |
| 1. Modelo de traducción | Gemma 4 E4B medido y validado como traductor, servido por llama-swap |
| 2. Servicio | API que entrega artículos traducidos y gestiona feeds |
| 3. App de Android | Leer en el móvil todo lo anterior |
| 4. Papers | Buscar en arXiv en español y guardar papers |
| 5a. App de PC: lectura y feeds | Leer y gestionar feeds en el portátil, con GPUI. Puede ir a la vez que la fase 3 |
| 5b. App de PC: papers y sin conexión | Lo que falta del PC, cuando estén las fases 3 y 4 |
| 6. Extras | Analizar paper completo, traducción local, chino y TranslateGemma |

---

## Fase 0. Preparación

**Reparto:** todo el orquestador, porque todo es `ai-lab`. Sin subagentes.

- [x] **0.1 Crear el monorepo.** Repositorio git, `AGENTS.md`, `CLAUDE.md`, `docs/` y `assets/`. Hecho cuando hay un primer commit con la documentación.
- [x] **0.2 Instalar Miniflux en `ai-lab`.** Miniflux y PostgreSQL en `~/servicios/docker-compose.yml`, con versión fija; Miniflux 2.2.16 o posterior (ADR 0005). Dominio `rss.rubensalas.dev` en el `Caddyfile` y en el DNS, solo para Tailscale (ADR 0008). Monitor en Uptime Kuma. Puertos, DNS y convenciones: los de `~/docs/ai-lab/ai-lab-stack.md`, que también se actualiza. Hecho cuando `https://rss.rubensalas.dev` abre desde el portátil y el móvil, y los demás servicios de `docker compose ps` siguen en marcha.
- [x] **0.3 Configurar Miniflux.** Crear el usuario y una clave de API, guardada en `~/servicios/.env`. Añadir el feed que Rubén ya sigue (pregúntale cuál) y activar en él la descarga del artículo completo si solo trae resumen. Hecho cuando `curl` con `X-Auth-Token` devuelve artículos de ese feed.

**Fase lista cuando:** Miniflux descarga el feed de Rubén y responde por su API.

---

## Fase 1. Elegir el modelo de traducción

Los textos y scripts de medida van en `bench/traduccion/`; los resultados, en `docs/traduccion.md`. Las medidas se hacen con la skill `/receta-modelo`.

**Reparto:** 1.1 un subagente. Después, 1.2 y 1.3 el orquestador, en orden, porque usan la GPU.

Se mide solo Gemma 4 E4B, en su versión `gemma-4-e4b-rapido` (sin razonamiento previo y con MTP). Si su calidad no basta, se añaden pasos para probar otro modelo; el siguiente candidato es Hunyuan-MT 7B en Q4_K_M.

- [x] **1.1 Preparar el conjunto de prueba.** 20 textos reales en inglés: 10 noticias y 10 resúmenes de arXiv, más 20 títulos sueltos. Un script que manda cada texto a un modelo de llama-swap y apunta tiempos. Hecho cuando el script corre de principio a fin contra un modelo cualquiera.
- [x] **1.2 Medir Gemma 4 E4B.** Ya está instalado en llama-swap como `gemma-4-e4b-rapido`. Medir memoria de GPU, tiempo de carga, segundos por título y por artículo. Hecho cuando sus medidas y sus 40 traducciones están en `docs/traduccion.md`.
- [x] **1.3 Decidir.** Rubén valora la calidad leyendo las traducciones; el agente presenta la tabla de medidas. Si la calidad basta, Gemma queda como modelo de traducción, con su `ttl` en `llama-swap/config.yaml`. Si no, se añaden al plan los pasos para medir otro modelo. Hecho cuando hay un ADR con la elección y el modelo responde por llama-swap, o cuando el plan tiene los pasos del siguiente candidato.

**Fase lista cuando:** un modelo elegido traduce por llama-swap y sus medidas están documentadas.

---

## Fase 2. Servicio de Atalaya

Crate `crates/atalaya-server`, con los tipos compartidos en `crates/atalaya-api` (ADR 0006).

**Reparto:** 2.1 el orquestador, porque despliega en `ai-lab`. Después, [2.2, 2.3, 2.5] en paralelo, y luego 2.4, que usa la traducción de 2.3. El orquestador despliega lo fusionado y hace las pruebas con `curl`.

- [x] **2.1 Esqueleto y despliegue.** Workspace de Cargo, los dos crates, un endpoint de salud, Dockerfile, entrada en `docker-compose.yml`, dominio `atalaya.rubensalas.dev` y monitor en Uptime Kuma. Además, los tipos de la API de toda la fase en `atalaya-api`, para que los pasos en paralelo partan del mismo contrato. Hecho cuando `https://atalaya.rubensalas.dev/health` responde desde el móvil.
- [x] **2.2 Listas desde Miniflux.** Listas, marcar leído y guardar, reenviados a Miniflux [L1, S1]. Todo protegido con una clave. Hecho cuando con `curl` se lista, se marca y el cambio se ve en la web de Miniflux.
- [x] **2.3 Traducir títulos.** Webhook de Miniflux para lo nuevo, detección de idioma, traducción de títulos por llama-swap y base SQLite de traducciones [T1–T4]. Hecho cuando un artículo nuevo aparece en la lista con el título en español en menos de un minuto.
- [x] **2.4 Traducir el texto completo.** Al abrir o guardar, por párrafos, conservando el HTML [T2, T5]. Si solo hay resumen, pedir antes el original a Miniflux [L7]. Mientras traduce, la API lo indica [L6]. Hecho cuando abrir un artículo en inglés devuelve el texto en español con sus enlaces intactos.
- [x] **2.5 Gestionar feeds.** Buscar los feeds de una web, añadir, editar, quitar, carpetas, feeds con error, idioma y artículos al día [F1–F6] (ADR 0007). Hecho cuando con `curl` se añade un feed a partir de `theverge.com` y aparece en Miniflux.

**Fase lista cuando:** todo lo que la app de Android necesita se puede hacer con `curl`.

---

## Fase 3. App de Android

En `android/`. La interfaz sigue `docs/diseño/maquetas.html`. Se prueba en el móvil por `adb`.

**Reparto:** 3.1 el orquestador, porque instala herramientas con `sudo` y usa el móvil. Después, [3.2, 3.4] en paralelo; luego 3.3 y luego 3.5, que tocan lo mismo que 3.2. 3.6, el orquestador con Rubén. Toda prueba en el móvil la hace el orquestador tras fusionar.

- [ ] **3.1 Proyecto y ajustes.** Herramientas de Android en el portátil, proyecto Kotlin con Compose, tema neutro claro y oscuro, icono adaptable de `assets/icono/` y pantalla de ajustes con «Probar» [S3]. Hecho cuando la app instalada en el móvil dice «Conectado» al servicio.
- [ ] **3.2 Lista y menú lateral.** [L1–L3, S2]. Hecho cuando la lista y el menú del móvil se parecen a las pantallas 1 y 2 de las maquetas, con datos reales.
- [ ] **3.3 Lectura y gestos.** Selector «Original», aviso «Traduciendo…» y gestos [L4–L6]. Hecho cuando se lee un artículo traducido y los gestos cambian su estado en Miniflux.
- [ ] **3.4 Feeds.** Pantallas 9 a 11 [F1–F6]. Hecho cuando se añade un feed desde el móvil y sus artículos llegan.
- [ ] **3.5 Sin conexión y en segundo plano.** Room para guardar lo descargado y WorkManager para sincronizar [L8, S4]. Hecho cuando, en modo avión, se leen los artículos ya sincronizados.
- [ ] **3.6 Uso real.** Rubén la usa un día. Se arreglan los fallos que encuentre. Hecho cuando Rubén confirma que la usa en lugar de su lector anterior.

**Fase lista cuando:** la app cubre los requisitos L, T, F y S en el móvil.

---

## Fase 4. Papers

**Reparto:** primero el orquestador añade a `atalaya-api` los tipos de la búsqueda. Después, en paralelo: un subagente para 4.1 y 4.2, en orden, y otro para 4.3.

- [ ] **4.1 Buscar en el servicio.** Traducir la búsqueda al inglés, consultar arXiv (Atom, una petición cada 3 segundos como mucho), traducir títulos [P1–P3] (ADR 0005). Hecho cuando con `curl` una búsqueda en español devuelve papers con títulos en español.
- [ ] **4.2 Abrir y guardar papers en el servicio.** Resumen traducido y guardado en el feed «Papers guardados» con «Import Entry» [P4, P5]. Hecho cuando un paper guardado aparece en Guardados con su resumen en español.
- [ ] **4.3 Pantallas de búsqueda en Android.** Pantallas 6 a 8 y búsquedas recientes [P1–P6]. Hecho cuando se busca, se abre y se guarda un paper desde el móvil.

**Fase lista cuando:** se busca «modelos de lenguaje pequeños», se guarda un paper y aparece traducido en Guardados.

---

## Fase 5a. App de PC: lectura y feeds

En `crates/atalaya-desktop`. La interfaz sigue `docs/diseño/maquetas-pc.html`. Solo depende del servicio de la fase 2, así que puede ir a la vez que la fase 3, en otra sesión y en la rama `fase-5a` (ver «Dos fases a la vez» en `AGENTS.md`).

**Reparto:** 5.1 un subagente. Después, [5.2, 5.4] en paralelo.

- [x] **5.1 Esqueleto con GPUI.** Versión de GPUI y `gpui-component` fijadas, ventana de tres columnas con la lista real. Hecho cuando la ventana muestra los artículos del servicio.
- [x] **5.2 Lectura.** Selector «Original», «Traduciendo…», leído y guardado [L1–L6]. Hecho cuando se lee y se guarda un artículo, y el cambio se ve en Miniflux.
- [ ] **5.4 Feeds y ajustes.** Pantallas 3 y 5 [F1–F6, S3]. Hecho cuando se añade un feed desde el PC.

**Fase lista cuando:** en el portátil se leen, guardan y marcan artículos, y se gestionan los feeds, igual que con el servicio por `curl`.

---

## Fase 5b. App de PC: papers y sin conexión

Empieza cuando estén hechas las fases 3 y 4: usa la búsqueda de papers de la fase 4 y la forma de sincronizar del paso 3.5. Rama `fase-5b`.

**Reparto:** 5.3 un subagente. 5.5 el orquestador, porque instala fuera del repo.

- [ ] **5.3 Papers.** Pantalla 4 de las maquetas [P1–P6]. Hecho cuando se busca y se guarda un paper desde el PC.
- [ ] **5.5 Sin conexión e instalación.** Caché en disco, entrada en el lanzador de Omarchy con su icono [L8]. Hecho cuando la app abre desde el lanzador y muestra lo ya sincronizado sin red.

**Fase lista cuando:** el PC y el móvil muestran lo mismo.

---

## Fase 6. Extras

Cada paso termina en una propuesta para Rubén, no en código. Si la aprueba, se escribe su ADR y se añaden pasos al plan.

**Reparto:** 6.1 un subagente. 6.2, 6.3 y 6.4 el orquestador, de uno en uno, porque usan el móvil y la GPU.

- [ ] **6.1 Paper completo.** Investigar la versión HTML de arXiv frente al PDF, y cómo mostrar fórmulas y tablas. Hecho cuando hay una propuesta con prueba sobre 3 papers reales.
- [ ] **6.2 Traducción local de respaldo.** Investigar ML Kit en Android. Hecho cuando hay una propuesta con calidad y tamaño medidos.
- [ ] **6.3 Chino al español.** Probar el modelo de la fase 1 con textos en chino. Hecho cuando hay una propuesta con 10 traducciones de ejemplo.
- [ ] **6.4 TranslateGemma 4B.** Medir esta Gemma ajustada para traducir con el conjunto de `bench/traduccion/`, igual que Gemma 4 E4B en el paso 1.2 (ADR 0009). Hecho cuando hay una propuesta con sus medidas y sus traducciones junto a las de Gemma 4 E4B en `docs/traduccion.md`.

---

## Bitácora

Una línea por sesión, la más reciente arriba. Se guardan las 10 últimas.

- 2026-10-23 · fase 2 · hecha · Servicio desplegado en atalaya.rubensalas.dev: listas, feeds, títulos por webhook y texto completo traducidos; copia SQLite y monitor listos. Siguiente: fases 3 y 5a.
- 2026-10-23 · fase 1 · hecha · Gemma 4 E4B medido y elegido (ADR 0009); Hunyuan y NLLB descartados; TranslateGemma pasa a extra 6.4. Siguiente: fase 2, paso 2.1.
- 2026-10-23 · fase 0 · hecha · Miniflux 2.3.4 en rss.rubensalas.dev, con clave de API y el feed de Paul Graham completo. Siguiente: fase 1, paso 1.1.
- 2026-10-23 · fase 0 · a medias · Hecho 0.1: diseño, PRD, ADR 0001–0008, plan y flujo con orquestador. Siguiente: 0.2, instalar Miniflux.
