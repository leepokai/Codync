# Prompt 00: Sincronizar con las 66 novedades de upstream

> Ola 0 · Depende de: ninguna (va primero) · ¿Puede correr en paralelo con otros de su ola?: no
> Base: `integrate/upstream-2.29` · Rama: `integrate/upstream-sync` · Worktree: `Codync-sync`

Pega este archivo completo al agente.

---

Eres un ingeniero senior de Rust (host), Kotlin/Jetpack Compose (Android), TypeScript/Electron (escritorio) y Swift/SwiftUI (iOS). Trabajas de principio a fin sin pedir confirmaciones; solo te detienes si algo es destructivo o si estás bloqueado, y entonces dices exactamente qué comando falló, su salida y qué intentaste.

## Dónde trabajar
- Repositorio: `C:\Users\Edwin Villeda\Documents\Codync` (remoto `origin` = mi fork Thndrr00/Codync).
- Crea un worktree NUEVO `C:\Users\Edwin Villeda\Documents\Codync-sync` con la rama NUEVA `integrate/upstream-sync` partiendo de `integrate/upstream-2.29`.
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
- Informe final en `docs/guides/upstream-sync-report.md` con: qué cambió por archivo, decisiones donde elegiste entre dos opciones y por qué, salida resumida de cada compuerta, lo que no pudiste probar (por ejemplo iOS) y los archivos compartidos que probablemente chocarán con otras ramas.
- Al terminar, un resumen de máximo 20 líneas.

## Objetivo de este prompt
Dejar `integrate/upstream-2.29` al día con `upstream/main` (hoy 2099c8a4, 66 commits más que v2.29.0) y traer los documentos de ideas, para que todos los demás prompts partan de una base común.

## Lee antes de empezar
- Informe de la fusión anterior: `docs/guides/upstream-2.29-merge.md` en la rama `integrate/upstream-2.29` (léelo para repetir su criterio de resolución).

## Pasos
1. `git fetch upstream --no-tags` y `git fetch origin`. Confirma con `git log --oneline a30c8f61..upstream/main` que son ~66 commits (PR #259 y anteriores: comando `send` que espera la respuesta, conexión de pantalla con trickle ICE, arreglos de iOS y del host).
2. Crea el worktree y la rama `integrate/upstream-sync` desde `origin/integrate/upstream-2.29` (commit 9a23b386). Activa `git config rerere.enabled true` solo en ese worktree.
3. Trae mis documentos de ideas con cherry-pick, uno a uno, en este orden: af33eda4, af10ea17, 35450a68, cd648e12, 20a9d74a (son solo archivos nuevos de `docs/`; si algún cherry-pick choca, resuélvelo conservando mi versión).
4. Fusiona `upstream/main` con un commit de merge (sin squash ni rebase). Resuelve por capas en este orden: host Rust, android/core, android/app y design, escritorio, iOS, docs y CI. Si upstream reescribió una pantalla, parte de su versión y vuelve a aplicar encima solo lo mío; si el archivo es solo mío, conserva el mío. No uses `-X ours/-X theirs` global. Cargo.lock y package-lock.json se regeneran con las herramientas, no a mano.
5. Lo mío que NO puede perderse (comprueba con `git grep` al final): `host/src/api/apps_bridge.rs` (`appRequest`), `host/src/proactive.rs` y su arranque en `background.rs`, `dispatch_bot_turn` en `api/apps_runtime.rs`, `MiniAppBridge.kt`, `MiniAppWebClient.kt`, `MiniAppBridgeScript.kt`, `DecisionActions` en `BotAppsCard.kt`, `collectPendingDecisions` con `allowOptionId`/`denyOptionId`/`takeoverRequest`, `ProactiveCheckIns.tsx`, el catálogo de bots y equipos y `docs/nuevo thndr/*`.
6. Atención a lo que upstream cambió cerca de mi trabajo: el comando `send` con espera y su manejo del tiempo límite de aprobaciones, y los cambios de `docs/features/approvals.md`.
7. Pasa todas las compuertas. Si alguna falla por un cambio de upstream, arréglala en un commit `fix:` aparte.
8. Sube `integrate/upstream-sync` a `origin`. No avances `integrate/upstream-2.29`; esa rama queda como fotografía de v2.29.0.

## Criterio de aceptación
- `git merge-base --is-ancestor upstream/main HEAD` verdadero; las compuertas en verde; la lista «lo mío que no puede perderse» comprobada; informe con conflictos resueltos por archivo.

## Si algo choca con otras ramas
Toca el mínimo de archivos compartidos y anota en el informe cuáles tocaste, para que el prompt 99 (cierre) los resuelva sin sorpresas.

No me pidas confirmación para nada de esto. Empieza ya.
