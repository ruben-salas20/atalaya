# 0012. El repo es público y las apps se actualizan solas desde GitHub Releases

- **Estado:** aceptada
- **Fecha:** 2026-10-25

## Contexto

Hasta ahora, cada versión de las apps la instalaba un agente: en Android con `adb` y en el PC con `scripts/instalar-pc.sh`. Si nadie la instala, Rubén se queda sin los arreglos. La #27 pide que las dos apps detecten una versión nueva y se actualicen solas.

El repo era privado, y las apps no pueden descargar de GitHub Releases (la página de versiones del repo) sin un token. Se valoraron tres opciones:

- Guardar un token de solo lectura en cada app. Un token en una app instalada se puede sacar, y caduca.
- Que el servicio de `ai-lab` reparta las versiones. No hace falta token, pero depende de que un agente compile y publique en `ai-lab`, y cambia Compose.
- Hacer público el repo y publicar en GitHub Releases. Es lo más simple y no depende de `ai-lab`.

Rubén eligió hacer público el repo. Antes se revisó toda la historia con `gitleaks` y a mano: no hay claves, tokens ni contraseñas (#50). Se quitó el texto completo de las noticias de prueba, que es de otros medios. La historia se publica tal cual, con el correo del autor de los commits.

## Decisión

- **Repo público.** Lo que entra en el repo es público, también la historia.
- **Versiones con etiqueta.** Cada versión es una etiqueta `vX.Y.Z` en `main`. La `version` del workspace de Cargo y `versionName` de Android dicen lo mismo. `versionCode` sale de la versión: `X*10000 + Y*100 + Z`.
- **Se compila en GitHub Actions.** Al subir la etiqueta, un flujo compila el APK firmado y el binario del PC, y los publica en la Release con su huella SHA-256 (una suma que permite comprobar que el archivo llegó entero). Las novedades van en español. Para repos públicos, Actions no cuesta nada.
- **Clave de firma fija.** Android solo instala una versión encima de otra si las dos llevan la misma firma. La clave de publicación se crea una vez, vive en los secretos de Actions y tiene una copia en el Vaultwarden de Rubén. Nunca entra en el repo.
- **Las apps preguntan al abrirse.** Leen la última Release en `https://api.github.com/repos/ruben-salas20/atalaya/releases/latest`, sin token, y la comparan con su versión.
  - **Android:** descarga el APK, comprueba la huella y se lo pasa al instalador de Android. Android pide confirmar con un toque. Si la app puede actualizarse sin preguntar, porque se instaló a sí misma, se usa.
  - **PC:** descarga el binario, comprueba la huella, sustituye `~/.local/bin/atalaya-desktop` y ofrece reiniciar.
- **Quién publica.** El orquestador pone la etiqueta tras fusionar un cambio que se nota en las apps.

## Consecuencias

- Rubén recibe cada arreglo sin pasos a mano. Basta un toque en el móvil.
- Nada privado puede entrar en el repo: ni secretos, ni datos de Rubén, ni textos ajenos completos. Se revisa con `gitleaks` antes de cada `push`.
- Pasar de las compilaciones actuales a las firmadas con la clave de publicación obliga a desinstalar la app del móvil una vez: hay que volver a escribir la dirección y la clave del servicio en Ajustes.
- Si se pierde la clave de firma, el móvil no acepta más versiones hasta desinstalar.
- Los nombres `*.rubensalas.dev` quedan a la vista. Solo resuelven dentro de Tailscale (ADR 0008), así que no abren nada.
- El servicio de `ai-lab` sigue desplegándose a mano, como hasta ahora.
- La #27 se divide en issues de código: versiones y flujo de Actions, actualización en Android y actualización en el PC.
