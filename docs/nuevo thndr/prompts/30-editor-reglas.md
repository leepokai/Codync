# Prompt 30: Editor de reglas de aprobación

> Ola 2 · Depende de: 11 (recomendado) · ¿Puede correr en paralelo con otros de su ola?: si
> Base: `integrate/upstream-sync` · Rama: `feat/approval-rules-editor` · Worktree: `Codync-pt1b`

Pega este archivo completo al agente.

---

Eres un ingeniero senior de Rust (host), Kotlin/Jetpack Compose (Android), TypeScript/Electron (escritorio) y Swift/SwiftUI (iOS). Trabajas de principio a fin sin pedir confirmaciones; solo te detienes si algo es destructivo o si estás bloqueado, y entonces dices exactamente qué comando falló, su salida y qué intentaste.

## Dónde trabajar
- Repositorio: `C:\Users\Edwin Villeda\Documents\Codync` (remoto `origin` = mi fork Thndrr00/Codync).
- Crea un worktree NUEVO `C:\Users\Edwin Villeda\Documents\Codync-pt1b` con la rama NUEVA `feat/approval-rules-editor` partiendo de `integrate/upstream-sync`.
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
- Informe final en `docs/guides/pt1b-rules-editor-report.md` con: qué cambió por archivo, decisiones donde elegiste entre dos opciones y por qué, salida resumida de cada compuerta, lo que no pudiste probar (por ejemplo iOS) y los archivos compartidos que probablemente chocarán con otras ramas.
- Al terminar, un resumen de máximo 20 líneas.

## Objetivo de este prompt
Una pantalla para crear, probar y ordenar las reglas de aprobación de cada bot en los cuatro clientes.

## Lee antes de empezar
- `C:\Users\Edwin Villeda\Documents\Codync\docs\nuevo thndr\guia-implementacion-ideas-parciales-y-nuevas-2026-10-28.md` (sección PT1b)
- `host/src/agent/approval_rules.rs`, `docs/features/approvals.md` («ningún cliente las edita todavía»), `host/src/api/bots.rs` (`updateBot`).

## Pasos
1. Mantén el contrato `{pattern, kind?, decision}` (100 reglas, 500 caracteres cada una).
2. Añade al host el método `testApprovalRule {botId, command, kind?}` que usa el MISMO código que `approval_rules.rs` y devuelve qué regla decide (o ninguna y por qué). Los clientes no copian lógica.
3. Pantalla en la ficha del bot (escritorio `BotEditorView.tsx`, Android `BotEditor.kt`, iOS, TUI): lista ordenada (el primer acierto decide), añadir, editar, borrar, reordenar, y un probador «escribe un comando».
4. Muestra la advertencia: un `allow` nunca aprueba comandos encadenados (`;`, `&&`, `||`, `|`, `` ` ``, `$(`, `>`, `<`, salto de línea).
5. Pruebas del método nuevo y de cada cliente; E2E: crear, probar y borrar una regla cambia el comportamiento de una aprobación real.

## Criterio de aceptación
- Crear, probar y borrar una regla en los cuatro clientes cambia el comportamiento de una aprobación real.

## Si algo choca con otras ramas
Toca el mínimo de archivos compartidos y anota en el informe cuáles tocaste, para que el prompt 99 (cierre) los resuelva sin sorpresas.

No me pidas confirmación para nada de esto. Empieza ya.
