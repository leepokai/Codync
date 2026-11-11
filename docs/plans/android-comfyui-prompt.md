# Prompt Android: ComfyUI en el teléfono y lo que el host ya ofrece

Para otro agente que continúe el trabajo de Android. Rama de partida: `feat/comfy-sidebar` (sobre `feat/openbot-marketplace` en `43c59885`). Todo lo de este documento sale de código y pruebas reales; lo que no se pudo comprobar está marcado como tal.

## 0. Reglas (las del proyecto, resumidas)

- **Solo añadir.** No borres, renombres ni reordenes ids, métodos del host, rutas de API, claves de traducción, componentes ni tests. Si algo parece sobrar, `git log -S` antes de tocarlo.
- **El host manda; el teléfono dibuja y envía intenciones.** Si falta un dato o una acción, se añade un método al host (Rust) y Android solo lo llama. Nada de lógica de ComfyUI en Kotlin.
- Componentes propios del módulo `design` (`CardSection`, `SecondaryButton`, `IconButton`, `Symbol`, `tr`), nunca Material más allá de `Text`.
- Todo cambio de UI en Android va también a escritorio y TUI en el mismo cambio (CLAUDE.md). Los textos nuevos, en los 10 catálogos de `apps/ios/Kit/Sources/CodyncKit/Resources/Languages` (se escriben con Node, conservando CRLF y sin BOM).
- No compiles APK. Verificación: `cd apps/android && ./gradlew :core:test :app:testDebugUnitTest :app:compileDebugKotlin`.

## 1. De dónde sale esto

| Pieza | Origen |
| --- | --- |
| El programa | Release oficial **Comfy-Org/ComfyUI v0.44.0** (commit `2c75dc1b379a59e0880619d029607af5169a62ef`), asset `ComfyUI_windows_portable_nvidia.7z`, 2 054 382 457 bytes, SHA-256 `7170c2db…8f96`. Fijado en un solo archivo: `packaging/comfy/version.json`. |
| Flags de arranque | `comfy/cli_args.py` de ese commit (`--listen`, `--port`, `--user-directory`, `--extra-model-paths-config`, `--cpu`…). |
| API que usa el host | `server.py`: `/system_stats`, `/object_info`, `/prompt`, `/queue`, `/history/{id}`, `/interrupt`, `/view`, `/upload/image`. Sin tokens: en esa versión el modo multiusuario no autentica, por eso corre en modo de un solo usuario. |
| Flujo de ejemplo | `script_examples/basic_api_example.py` (idéntico, `host/src/tools/comfy_example.json`). |
| El logo | `comfy-logo-mono.svg` del paquete `comfyui_frontend_package` 1.57.6 que viene dentro del portable, sin su cuadro redondeado, reescalado a 24 puntos. Mismo trazado en escritorio (`icon-fallbacks.ts`, clave `comfyui.logo`) y Android (`design/src/main/res/drawable/comfyui_logo.xml`). |
| El patrón | `host/src/n8n.rs` y `apps/desktop/src/main/n8n.ts`; planes en `docs/plans/bundled-tools-production.md` y `design-inside-codync.md` (están sin commitear en el checkout principal). |
| Documentación | `docs/reference/comfyui.md` (fuentes, diferencias con el plan) y `web/content/docs/comfyui.mdx` (manual de usuario). |

Ramas: `feat/comfy-bundled` (primer trabajo, 8 commits) y `feat/comfy-sidebar` (la anterior más el logo en la barra lateral, Android, TUI y la descarga en el primer uso). Último commit al escribir esto: `2d6445c0`.

## 2. Lo que ya está en el host (no lo rehagas)

`host/src/tools/`: `runner.rs` (proceso en Job Object, puerto libre, estados), `health.rs`, `log.rs`, `lifecycle.rs` (arranque con el host, parada sin huérfanos), `comfy.rs` (rutas, carpetas, comprobaciones, lanzador), `comfy_install.rs` (descarga en el primer uso), `comfy_api.rs`, `comfy_job.rs` (un trabajo a la vez), `comfy_mcp.rs` (herramientas para los bots).

Métodos de API para clientes: `toolStatus`, `toolStart`, `toolStop` (todos con `{"id":"comfy"}`), `comfyCancel`, `comfyJobImage`. `comfyCall` (puente MCP de los bots) es **solo local**: un teléfono no puede llamarlo (403 en el canal remoto, con prueba).

`toolStatus` devuelve, entre otros: `state` (`stopped|starting|running|error`), `error`, `port`, `url`, `logTail`, `comfy` (`version`, `bundled`, `installed`, `downloadBytes`, `usedBytes`, `modelsDir`, `notice` si corre en procesador), `job` (`phase` `queued|running|done|cancelled|error`, `files`) e `install` (`phase` `idle|downloading|unpacking|failed`, `done`, `total`, `error`).

Eventos por SSE/canal: `{"type":"tool","id":"comfy"}` y `{"type":"toolJob",...}`. **Android los ignora hoy** (`Conversation.kt` → `else -> Ignored`), así que sondea cada 3 s.

## 3. Lo que hice en Android

- `design/.../drawable/comfyui_logo.xml` y su alta en `Symbol.kt` (`"comfyui.logo"`).
- `app/.../AppStoreComfy.kt`: `ComfyStatus` (lectura de `toolStatus`, con valores por defecto para un host antiguo) y `comfyStatus/Start/Stop/Cancel`.
- `app/.../ComfySection.kt`: tarjeta «ComfyUI» en Ajustes, bajo Browser (`SettingsView.kt`, una línea). Logo, estado, Iniciar/Detener, aviso de procesador, línea del trabajo con Cancelar, y el flujo de primera descarga (tamaño, «Descargando… X de Y», «Descomprimiendo…», error, «Descargar e iniciar»).
- `app/src/test/.../ComfyStatusTest.kt`: lectura de la respuesta del host (incluye host antiguo, build sin ComfyUI y descarga).
- Reutiliza `HostHealth.bytesText` de `core` para los tamaños.

**Sin verificar:** no se compiló nada de Android (se pidió expresamente). Puede haber errores de compilación o de imports. Primer paso: correr el Gradle de arriba y corregir.

## 4. Mejoras para ComfyUI en Android (ideas, de menor a mayor esfuerzo)

1. **Ver la imagen que acaba de salir.** `comfyJobImage` ya devuelve la imagen como data URL; la tarjeta de escritorio la enseña, la de Android no. Solo falta pedirla cuando `job.phase == "done"` y pintarla.
2. **Eventos en vez de sondeo.** Tratar `tool`/`toolJob` en `Conversation.kt` y refrescar al momento.
3. **Aviso al terminar.** El host ya manda alertas push («necesita tu atención», «terminó», «falló»). Añadir en el host una alerta «Imagen lista / falló» al acabar un trabajo, y que Android la muestre con la miniatura. Requiere método nuevo o gancho en `comfy_job.rs`.
4. **Cancelar desde la notificación** del trabajo en curso (`comfyCancel`).
5. **Progreso real** en una Live update de Android 16 (`ProgressStyle` ya existe en el proyecto): hoy el host solo sabe `queued/running`; ComfyUI emite `progress` por WebSocket, el host tendría que leerlo y exponerlo.
6. **Galería de resultados.** Método nuevo en el host (solo lectura, permitido a teléfonos) que liste `~/.codync/comfy/output` y permita enviar una imagen al chat de un bot como adjunto.
7. **Gestor de modelos de solo lectura.** Método nuevo que liste checkpoints/LoRAs instalados, el espacio usado y el enlace de descarga (hoy solo lo ve el bot por `list_models`, que es local).
8. **«Crear imagen» desde el teléfono.** Cuadro de texto → método nuevo que arma el flujo de ejemplo con ese texto y el checkpoint elegido y lo encola. Con foto de la cámara como imagen de entrada (`upload_image`).
9. **Activar ComfyUI para un bot** desde el editor del bot: el conector `comfyui` aparece en la lista de conectores cuando ComfyUI corre; comprobar que Android lo muestra y lo conmuta (no lo probé).

Reglas para 3, 6, 7 y 8: método nuevo en el host con su prueba, decidir qué ámbito (`scopes`) lo permite a un teléfono, y recordar que `comfyCall` nunca se abre.

## 5. Qué más tiene el host y Android aún no usa

Comprobado con búsqueda de cadenas: el nombre del método **no aparece** en `apps/android` (los marcados con * se buscaron solo en `app/src/main`; confirma también en `core/` antes de empezar).

| Función del host | Métodos | Nota |
| --- | --- | --- |
| Red de Codync (bots de otros ordenadores) | `networkStatus`, `networkPeers`, `networkCreate`, `setNetworkRules` (comprobados: ni una coincidencia en `apps/android/app/src/main`); `networkInvite`, `networkJoin`, `networkLeave`, `networkPromote`, `networkRecover`, `networkRemove` (solo vistos en la lista del host, sin buscarlos en Android) | El escritorio tiene su lógica (`store/network.ts`) y el TUI su gestión (`tui/manage/network.rs`): úsalos como referencia de qué mostrar. Es el hueco mayor. |
| Historial de instrucciones de un bot | `instructionHistory`, `instructionDiff`, `restoreInstructions` | Ver cambios y restaurar una versión. |
| Calendario de rutinas | `routineCalendar` | Vista de las próximas ejecuciones. |
| Equipos con coordinador* | `setLead`, `setAssistants` | |
| WhatsApp* | `whatsappSend`, `whatsappAllowed` | Android ya enlaza el número; faltan enviar y la lista de chats permitidos. |
| Conversación y memoria* | `keepConversation`, `importAgentMemory`, `messageResult`, `editHandoff` | |
| Habilidades* | `importSkill` | `skillDetails` ya se usa. |
| Aprobaciones y secciones* | `setApproval`, `renameSection` | Reglas de aprobación y renombrar secciones del roster. |
| Documentos* | `reviewDocument` | |
| Temas* | `themeThemes` | Las paletas de tema existen en escritorio. |
| Salud del host* | `setHealthThresholds` | `hostHealth` y `sentinelHealth` ya se leen. |
| Composio* | `composioDispatchCall` | |

Ya los usa Android (no los rehagas): `doctor`, `tokenBudgets`, `setTokenBudget`, `sentinelHealth`, `auditLog`, `clearFailedRuns`, `retireDormantSkills`, `exportBot`, `importBots`, `voiceModels`, `botConversation`, `reorderBots`, `newSession`, `deleteEntry`, `skillDetails`.

Los métodos de `LOCAL_ONLY` (`host/src/api/devices.rs`: pantalla, claves de voz, importar desde archivo, pairing, `*Call` de puentes MCP…) **no** se pueden llamar desde un teléfono por diseño: no los incluyas.

## 6. Cosas que aprendimos y te van a ahorrar tiempo

- **NSIS (32 bits) no puede llevar ComfyUI dentro** (2,5 GB comprimido; error `failed creating mmap`). Por eso se descarga en el primer uso: `comfy_install.rs`, con 7za junto al host.
- **Windows Defender** puede tener abiertos los 62 000 archivos recién descomprimidos y el movimiento final fallaba con «Acceso denegado»; ya reintenta durante un minuto y reutiliza el archivo ya verificado.
- **`electron-builder.dev.yml` reemplaza** las listas de recursos del base: cualquier recurso nuevo va en los dos.
- **El host de desarrollo es compartido.** Otro agente puede levantar su propio `codync-host` en el puerto 19223 y desplazar el tuyo (a mí me pasó: el host que contestaba era el de `Deska\worktrees\android-openbot-parity`, de una base sin `toolStatus`, y devolvía 404). Comprueba `binaryPath` en `GET /health` antes de culpar al código, y no cierres procesos que no sean tuyos.
- **El disco se llena:** ComfyUI pide 10 GB libres en el primer arranque y descargar necesita 8 GB; el programa ocupa 4,5 GB desempaquetado.
- **Sin GPU NVIDIA** corre en procesador y el host lo avisa en `comfy.notice`: 2 minutos para arrancar, minutos por imagen.

## 7. Pruebas que existen (para no repetirlas y para ampliarlas)

- Host: `cargo test --bin codync-host -- tools:: tui:: devices remote::channel` (202 pasan). Dos pruebas `#[ignore]` arrancan el ComfyUI real: `CODYNC_COMFY_DIR=<portable desempaquetado> CODYNC_HOME=<carpeta vacía> cargo test real_comfyui -- --ignored --test-threads=1`.
- Escritorio: `npm run typecheck && npm test` (532 pasan, incluye `src/shared/tools.test.ts`).
- E2E en Windows: `npm run dist:win:dev`, cerrar solo `Codync Dev` y su host, `.\dist\codync-dev-windows-x64.exe /S`, abrir con `--remote-debugging-port=NNNN` y conducir con `node tools/cdp-eval.mjs <puerto> "<expr>"`.
- Android: solo `ComfyStatusTest.kt`, sin ejecutar.

## 8. Qué no está hecho (sé honesto con la persona al informar)

- Android sin compilar ni ejecutar.
- Un trabajo real con imagen generada por un modelo (no hay ningún checkpoint descargado) y la imagen llegando al chat de un bot en marcha.
- La matriz de la fase 6 del plan: Windows limpio, AMD, ComfyUI propio instalado aparte, reinicio del equipo, actualizar y desinstalar con y sin conservar datos.
- Confirmar en la app que, tras la primera descarga, la ventana pasa a `running` y carga el editor: el log de ComfyUI llegó a «Starting server… 127.0.0.1:8188», pero el host de otro agente desplazó al mío antes de poder verlo.
