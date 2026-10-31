# Proveedores de traducción en la nube y web de ajustes: propuesta

Propuesta de la issue #104, escrita el 2026-10-31. Falta medir la calidad: hace falta una clave de un proveedor en la nube (ver «Qué falta»).

## Resumen

- **Coste:** con el uso actual, cualquier modelo pequeño o mediano en la nube costaría **menos de 2 USD al mes**. Uno de los grandes, unos 12 USD.
- **Cambio en el servicio:** es pequeño. El servicio ya usa el formato de OpenAI. Falta mandar la clave, tratar el error 429 y guardar la configuración en la base en vez de en el `.env`.
- **Privacidad:** con OpenRouter se puede exigir, en cada petición, que el proveedor no guarde los textos ni entrene con ellos.
- **Seguridad:** la web vive en el mismo servicio, en `/ajustes`, solo dentro de Tailscale. Se entra con la clave de la API. La clave del proveedor se guarda cifrada y no se vuelve a mostrar.
- **Decisiones de Rubén:** sale del ADR 0002 («toda la traducción ocurre en `ai-lab`») y del 0008 («el servicio no muestra ninguna web»). Si se aprueba, hacen falta dos ADR nuevos.

## Uso real

Datos del servicio el 2026-10-31, tras 8 días en marcha:

- 246 artículos vistos: unos 30 títulos al día.
- 6 artículos y 13 papers con la traducción guardada. El cuerpo de un artículo se traduce solo al abrirlo.
- Un paper completo genera de 19 000 a 25 000 tokens de salida. El de matemáticas, 108 000 (`docs/paper-completo.md`).

Un **token** es un trozo de palabra. Los proveedores cobran por millón de tokens de entrada (lo que se manda) y de salida (lo que el modelo escribe).

Mes supuesto, con margen sobre el uso real:

| Qué | Cuántos | Entrada | Salida |
|---|---|---|---|
| Títulos | 900 | 90 000 | 27 000 |
| Artículos abiertos | 60 | 60 000 | 54 000 |
| Resúmenes de arXiv | 30 | 12 000 | 14 000 |
| Papers completos | 10 | 250 000 | 250 000 |
| **Total** | | **~420 000** | **~345 000** |

## Coste estimado

Precios públicos de OpenRouter (`https://openrouter.ai/api/v1/models`) consultados el 2026-10-31, en USD por millón de tokens. Coste = total del mes por el precio.

| Modelo | Entrada | Salida | Coste al mes |
|---|---|---|---|
| `google/gemma-4-31b-it` (misma familia que el local, 8 veces más grande) | 0,20 | 0,40 | 0,22 USD |
| `anthropic/claude-haiku-5.5` | 0,10 | 0,50 | 0,21 USD |
| `deepseek/deepseek-v4.1-flash` | 0,045 | 0,50 | 0,19 USD |
| `google/gemini-3.1-flash-lite` | 0,25 | 1,50 | 0,62 USD |
| `openai/gpt-5.4-mini` | 0,75 | 4,50 | 1,87 USD |
| `openai/gpt-5.5` | 5,00 | 30,00 | 12,45 USD |

Los papers completos son el 70 % del coste. Si Rubén lee más papers, el coste sube casi en proporción.

## Qué cambia entre proveedores

| | llama-swap | OpenAI | OpenRouter | Groq |
|---|---|---|---|---|
| Ruta `/v1/chat/completions` | ✓ | ✓ | ✓ | ✓ |
| Pide cabecera `Authorization: Bearer` | — | ✓ | ✓ | ✓ |
| Acepta `max_tokens` | ✓ | — | ✓ | ✓ |
| Límite de peticiones con error 429 | — | ✓ | ✓ | ✓ |
| Exigir que no guarde ni entrene, por petición | — | — | ✓ | — |

- **`max_tokens`:** los modelos de razonamiento de OpenAI (familia GPT-5) piden `max_completion_tokens` y solo aceptan la temperatura por defecto. El servicio debe poder elegir el nombre del campo y omitir la temperatura.
- **Razonamiento:** un modelo que «piensa» gasta tokens de salida antes de responder. Hay que pedirle el esfuerzo mínimo, o elegir un modelo sin razonamiento, para que no corte la traducción ni suba el coste.
- **Error 429:** hoy el servicio solo reintenta los 502 y 503 de llama-swap. Con un proveedor en la nube debe esperar lo que diga la cabecera `retry-after` y reintentar.
- **Groq gratis no sirve para papers:** su nivel gratuito permite unos 8000 tokens por minuto en `gpt-oss-120b` (`console.groq.com/docs/rate-limits`). Un paper completo tardaría más que con Gemma local.
- **Respaldo:** si el proveedor en la nube falla o se queda sin saldo, el servicio vuelve a llama-swap. Así nada se queda sin traducir.

## Privacidad

Los textos de los artículos saldrían de `ai-lab`. Son artículos públicos, pero también revelan qué lee Rubén.

- **OpenRouter** no guarda los textos por defecto, salvo que se active el registro o el contenido salte su filtro de abuso infantil (`openrouter.ai/docs/guides/privacy/data-collection`). Con `"provider": {"data_collection": "deny", "zdr": true}` en cada petición, solo usa proveedores que no recogen datos y no los retienen (*zero data retention*, ZDR).
- **OpenAI** y **Groq** directos: no comprobé hoy sus políticas actuales; no las doy por buenas.

Recomendación: OpenRouter, con esas dos opciones fijas en el código.

## Web de ajustes

- **Dónde:** en el mismo servicio en Rust, en `/ajustes`, con HTML sencillo generado por el servidor y sin framework. No hay otro contenedor que vigilar.
- **Quién entra:** un formulario pide la clave de la API del servicio. Si es correcta, el servicio pone una cookie de sesión `HttpOnly`, `Secure` y `SameSite=Strict`, que caduca a los 30 días. Los formularios llevan un código anti-CSRF (un valor que impide que otra web los envíe en nombre de Rubén).
- **Qué se configura:** URL base, modelo y clave del proveedor. Uno para títulos y otro para textos y papers. llama-swap queda siempre como respaldo. Un botón «Probar» traduce un título de ejemplo y muestra el tiempo y el resultado.
- **La clave del proveedor:**
  - se guarda cifrada en la base, con ChaCha20-Poly1305 y una clave maestra nueva en el `.env` de `ai-lab` (`ATALAYA_SECRET_KEY`);
  - tras guardarla, la web solo muestra «configurada» y sus 4 últimos caracteres;
  - nunca sale en los registros, ni en los errores del proveedor que se registran.
- **Red:** sigue solo dentro de Tailscale (ADR 0008). Lo que cambia del 0008 es que el servicio sí muestra una web.

## Qué falta

1. **Medir la calidad.** Comparar Gemma local con dos modelos en la nube en `bench/traduccion` (las 10 noticias) y en un paper completo: párrafos en inglés, marcas rotas y tiempo. Hace falta una clave de OpenRouter con un tope de gasto bajo; la medida cuesta menos de 1 USD.
2. **Decisión de Rubén** sobre la propuesta. Si la aprueba, se escriben los ADR y se abren las issues de código:
   - el servicio habla con proveedores con clave, 429 y respaldo a llama-swap;
   - configuración del proveedor en la base, cifrada;
   - web `/ajustes` con sesión y «Probar».
