# Prompt 99: Cierre: integrar todo y verificar

> Ola 5 · Depende de: todos los anteriores que quieras incluir · ¿Puede correr en paralelo con otros de su ola?: no
> Base: `integrate/features-1` · Rama: `integrate/final` · Worktree: `Codync-final`

Pega este archivo completo al agente.

---

Eres un ingeniero senior de Rust (host), Kotlin/Jetpack Compose (Android), TypeScript/Electron (escritorio) y Swift/SwiftUI (iOS). Trabajas de principio a fin sin pedir confirmaciones; solo te detienes si algo es destructivo o si estás bloqueado, y entonces dices exactamente qué comando falló, su salida y qué intentaste.

## Dónde trabajar
- Repositorio: `C:\Users\Edwin Villeda\Documents\Codync` (remoto `origin` = mi fork Thndrr00/Codync).
- Crea un worktree NUEVO `C:\Users\Edwin Villeda\Documents\Codync-final` con la rama NUEVA `integrate/final` partiendo de `integrate/features-1`.
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
- Informe final en `docs/guides/final-integration-report.md` con: qué cambió por archivo, decisiones donde elegiste entre dos opciones y por qué, salida resumida de cada compuerta, lo que no pudiste probar (por ejemplo iOS) y los archivos compartidos que probablemente chocarán con otras ramas.
- Al terminar, un resumen de máximo 20 líneas.

## Objetivo de este prompt
Juntar en una sola rama las ramas terminadas, pasar todas las compuertas y dejar un informe de release sin subir versión.

## Lee antes de empezar
- Las ramas `feat/*` y `fix/*` listadas en `docs/nuevo thndr/prompts/00-INDICE.md`.

## Pasos
1. Crea `integrate/final` desde `integrate/features-1` (o `integrate/upstream-sync` si el prompt 01 no se hizo).
2. Fusiona, una a una y con commit de merge propio, las ramas que estén terminadas en este orden: `fix/checkins-and-webview`, `feat/shell-blocklist`, `feat/approvals-stuck`, `feat/turn-verdict`, `feat/verify-gate`, `feat/approval-rules-editor`, `feat/always-rule`, `feat/action-rules`, `feat/security-profiles`, `feat/token-saver`, `feat/external-questions-host` y `-ui`, `feat/channels-contract`, `feat/channel-telegram`, `feat/channels-ui`, `feat/host-health`, `feat/health-ui`, `feat/agent-import`, `feat/weekly-recap`, `feat/demo-mode`, `feat/memory-beliefs-host` y `-ui`, `feat/skill-proposals-host` y `-ui`, `feat/ios-features-1`. Omite las que no existan en `origin`.
3. Tras cada fusión pasa las compuertas de la capa tocada; arregla en commits `fix:` aparte. Resuelve choques entre ramas con el criterio de «partir de lo más reciente y reaplicar lo otro»; anota cada decisión.
4. Al final pasa las tres compuertas completas y una prueba de extremo a extremo de los flujos principales (aprobaciones, canales con servidores falsos, rutinas por nodos, miniapps y check-ins).
5. Construye el APK firmado y el instalador de Windows de la rama; instala el APK en el teléfono (te aviso del MIUI) y haz una pasada de humo.
6. Escribe `docs/guides/release-notes-final.md` con lo nuevo por área, lo que NO se probó (iOS sin compilar, Slack real, etc.) y la versión propuesta (2.30.0 o la que corresponda), SIN subirla.

## Criterio de aceptación
- `integrate/final` en `origin` con todas las compuertas en verde; informe con ramas fusionadas, omitidas y por qué.

## Si algo choca con otras ramas
Toca el mínimo de archivos compartidos y anota en el informe cuáles tocaste, para que el prompt 99 (cierre) los resuelva sin sorpresas.

No me pidas confirmación para nada de esto. Empieza ya.
