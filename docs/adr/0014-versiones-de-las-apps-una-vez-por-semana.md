# 0014. Las apps salen como mucho una vez por semana, con el número de la etiqueta

- **Estado:** aceptada
- **Fecha:** 2026-10-25

## Contexto

Con la actualización automática (ADR 0012), cada cambio en una app sería una versión nueva y un aviso más para Rubén. El ADR 0012 decía que el orquestador pone la etiqueta tras fusionar cualquier cambio que se note en las apps, y que el número de versión vive en `Cargo.toml`, así que cada versión pedía además un PR solo para subirlo.

Se valoró que las apps se actualizaran en silencio. Rubén lo descartó: quiere ver y confirmar cada actualización.

## Decisión

- **Servicio, al momento.** Lo que cambia en el servicio se despliega tras fusionar (ADR 0013). No hace falta versión de las apps.
- **Apps, una vez por semana.** Los domingos, si cambió algo de las apps (`android/`, `crates/atalaya-desktop/` o `crates/atalaya-api/`) desde la última etiqueta, el orquestador publica la versión siguiente con sus novedades. Si nada cambió, no hay versión.
- **Urgencias.** Si una app está rota de verdad, el arreglo sale el mismo día.
- **El número sale de la etiqueta.** El flujo «Publicar versión» escribe la versión de la etiqueta en `Cargo.toml` y `Cargo.lock` con `scripts/poner-version.sh` antes de compilar. `main` no cambia: su `Cargo.toml` puede quedar por detrás.
- **Se ve.** Las apps siguen avisando y piden «Actualizar», como dice el ADR 0012.

Esto sustituye, en el ADR 0012, la línea «Quién publica» y la regla de que la versión del repo y la de la etiqueta coinciden.

## Consecuencias

- Rubén ve como mucho un aviso por semana.
- Un arreglo de una app puede tardar hasta una semana en llegarle, salvo que sea urgente.
- Publicar es solo poner una etiqueta anotada con las novedades: no hace falta PR.
- Las compilaciones locales de `main` dicen la versión de `Cargo.toml`, que puede ser la anterior a la publicada.
