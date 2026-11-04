# Prompt 03: Arreglos del commit de miniapps y check-ins

> Ola 1 · Depende de: 00 · ¿Puede correr en paralelo con otros de su ola?: si
> Base: `integrate/upstream-sync` · Rama: `fix/checkins-and-webview` · Worktree: `Codync-fixes`

Pega este archivo completo al agente.

---

Eres un ingeniero senior de Rust (host), Kotlin/Jetpack Compose (Android), TypeScript/Electron (escritorio) y Swift/SwiftUI (iOS). Trabajas de principio a fin sin pedir confirmaciones; solo te detienes si algo es destructivo o si estás bloqueado, y entonces dices exactamente qué comando falló, su salida y qué intentaste.

## Dónde trabajar
- Repositorio: `C:\Users\Edwin Villeda\Documents\Codync` (remoto `origin` = mi fork Thndrr00/Codync).
- Crea un worktree NUEVO `C:\Users\Edwin Villeda\Documents\Codync-fixes` con la rama NUEVA `fix/checkins-and-webview` partiendo de `integrate/upstream-sync`.
- Trabaja SOLO en ese worktree. No modifiques ningún otro worktree, rama ni carpeta (ni `Codync`, ni `Codync-windows-validation`, ni los de Deska). Solo puedes LEER, sin modificar, los documentos y los clones de referencia que se indican en «Lee antes de empezar».

## Reglas fijas (no negociables)
- Lee primero `CLAUDE.md`, `AGENTS.md` y `docs/guides/clean-code.md` del repo. Ignora de ellas la parte de fusionar a `dev`/`main`, de subir versión y de crear tags: este fork integra por ramas.
- El host (Rust 2024) es dueño de la lógica; los clientes muestran y mandan intenciones. Rust: sin `unwrap()` fuera de pruebas, enums para estados, `tracing` con campos, trabajo bloqueante en `spawn_blocking`, ningún archivo nuevo de más de ~500 líneas, clippy pedantic con `-D warnings`.
- Todo cambio de interfaz va a los CUATRO clientes en el mismo cambio: escritorio (`apps/desktop/src/renderer`), Android (`apps/android/app`), iOS (`apps/ios`) y terminal (`host/src/tui`). Si uno no aplica, lo documentas con el motivo. iOS no se puede compilar en esta PC: lo escribes con cuidado y lo marcas «no compilado».
- Sin `--force`, sin borrar ramas ni worktrees, sin subir versión, sin crear tags, sin abrir PR contra el repositorio original.
- Si copias código de `C:\Users\Edwin Villeda\Documents\ref-repos` (solo lectura): respeta su licencia (MIT; Apache-2.0 en Rakazo, con `NOTICE` y archivo marcado como modificado), deja el origen en la cabecera del archivo y añádelo al archivo de atribuciones de terceros. Nunca uses el nombre, el arte ni los sonidos de Coucou.
- Nunca copies al repositorio claves ni contraseñas. La firma de Android usa `C:\Users\Edwin Villeda\.codync-signing\codync-release.jks` (alias `codync`, contraseña en `keystore-password.txt` de esa carpeta) con `C:\Android\Sdk\build-tools\36.0.0\apksigner.bat`; nunca la clave de depuración.

## La máquina (8 GB de RAM, poco disco)
- Antes de compilar comprueba >20 GB libres en C:; si baja de 8 GB borra solo `host/target/debug/incremental`. No mates procesos ajenos.
- Cargo: `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1` (el host entero tarda ~20 min). Gradle: `--no-daemon -Dorg.gradle.jvmargs=-Xmx2g`; haz `./gradlew.bat --stop` antes de compilar Rust y no compiles Rust y Gradle a la vez.
- electron-builder: genera antes `apps/desktop/dist/icon.ico` (PNG 256x256 dentro de un ICO) y usa `npx electron-builder --win -c.win.icon=dist/icon.ico`.
- Instalar un APK por USB en el Xiaomi exige que yo acepte el aviso de MIUI: avísame en ese paso y espera.
- Las pruebas de extremo a extremo usan el host de desarrollo (puerto 19223, carpeta `~\.codync-dev` o una temporal), NUNCA el de producción (puerto 19222).

## Compuertas (no avances con fallos; «pasa» = viste código de salida 0)
- Host: `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test -j 1 -- --test-threads=1` (si la suite entera no cabe en memoria, al menos los módulos que tocaste y `api`, `store`, `routines`).
- Escritorio (`apps/desktop`): `npm run typecheck` y `npm test`.
- Android (`apps/android`): `./gradlew.bat assembleDebug testDebugUnitTest :core:test lintDebug --no-daemon -Dorg.gradle.jvmargs=-Xmx2g`.
- Prueba real del flujo cambiado, con capturas cuando haya pantalla.

## Cómo reportar
- Commits pequeños con mensajes `feat(...)`/`fix(...)`/`docs(...)`; sube la rama a `origin` (sin force) al cerrar cada fase.
- Informe final en `docs/guides/fix-checkins-webview-report.md` con: qué cambió por archivo, decisiones donde elegiste entre dos opciones y por qué, salida resumida de cada compuerta, lo que no pudiste probar (por ejemplo iOS) y los archivos compartidos que probablemente chocarán con otras ramas.
- Al terminar, un resumen de máximo 20 líneas.

## Objetivo de este prompt
Corregir los fallos que encontré al revisar el commit `b6bb8228` (miniapps remotas, check-ins proactivos, decisiones pendientes) y una brecha de seguridad del escritorio.

## Lee antes de empezar
- Archivos: `host/src/proactive.rs`, `host/src/api/apps_runtime.rs` (`dispatch_bot_turn`), `host/src/api/apps_bridge.rs`, `apps/android/app/.../MiniAppWebClient.kt`, `MiniAppBridgeScript.kt`, `BotAppSheet.kt`, `apps/desktop/src/main/index.ts`.

## Pasos
1. **Check-ins visibles:** hoy el texto del check-in entra al chat como un mensaje de usuario visible y la respuesta `NO_UPDATE` no se filtra, lo que contradice «solo te escribe si tiene algo que decir». Marca esa entrada como interna (campo `from: "proactive"` ya existe) y ocúltala en los CUATRO clientes; descarta las respuestas cuyo texto, recortado, sea exactamente `NO_UPDATE` (ocultas y sin notificación `AlertKind::Done`). Añade pruebas en el host.
2. **WebView de miniapps (Android):** en `MiniAppWebClient` implementa `shouldOverrideUrlLoading` para que todo lo que no sea el origen virtual `https://codync-app.invalid` se abra en el navegador del sistema; el puente `CodyncBridge` solo debe atender a la página principal de ese origen. Prueba en `core`.
3. **`fetch` del puente:** hoy pierde en silencio los cuerpos que no son texto (`FormData`, `Blob`, `Request` con cuerpo). Haz que lance un error claro o que los soporte; añade prueba.
4. **Host:** corrige el `needless_borrow` de `proactive.rs` (`key(&id)`, `hub.runtime(&id)` sobre un `&String`) y el `kv_delete(...).ok()` que traga errores (usa `tracing::warn!` con campos). Añade pruebas de `appRequest`: token inválido = 403, ruta con `..` = 400, página, `_data` POST y GET.
5. **Escritorio:** `apps/desktop/src/main/index.ts` (alrededor de la línea 91) abre con `shell.openExternal` cualquier URL de `window.open`. Aplica la misma validación de esquemas que ya tiene `app:openExternal` (`https?`, `mailto`) y añade prueba.
6. E2E con el host de desarrollo: activa check-ins con intervalo de 1 hora y verifica el libro en `kv` (`proactive:<botId>`), que no aparece mensaje visible ni `NO_UPDATE` en el chat y que no hay notificación espuria.

## Criterio de aceptación
- Compuertas en verde; ningún check-in deja mensajes visibles; los enlaces externos del WebView y de `window.open` ya no cargan esquemas peligrosos.

## Si algo choca con otras ramas
Toca el mínimo de archivos compartidos y anota en el informe cuáles tocaste, para que el prompt 99 (cierre) los resuelva sin sorpresas.

No me pidas confirmación para nada de esto. Empieza ya.
