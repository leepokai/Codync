# Guía para implementar las ideas parciales y nuevas (2026-10-28)

Guía paso a paso para llevar a Codync las ideas que quedaron en
`ideas-30-por-repo-como-integrarlas-2026-10-28.md`, `analisis-ideas-6-repos-2026-10-28.md` e
`ideas-nuevas-y-grandes-2026-10-28.md`. Cada paquete de trabajo (PT) es una rama independiente con su
objetivo, el estado actual en el código, el código de referencia, los pasos, las pruebas y el criterio de
aceptación.

Los nombres de archivo de Codync de esta guía están comprobados en el repositorio. Donde dice
«verificar», el detalle depende de algo que no pude confirmar leyendo el código.

## 0. Reglas generales para todos los paquetes

### 0.1 Preparación
1. **Rama base:** cuando termine la fusión con v2.29.0, `integrate/upstream-2.29`. Mientras tanto,
   `integrate/windows-all-additions` (cd648e12 o posterior).
2. **Un worktree por paquete:** `git worktree add ..\Codync-pt<N> -b feat/<nombre> <base>`. Una rama por paquete;
   sin `--force`; sin subir versión ni crear tags (la versión se decide al fusionar a `main`).
3. **Lectura previa:** `CLAUDE.md`, `AGENTS.md`, `docs/guides/clean-code.md` y el `docs/features/*.md` del tema.

### 0.2 Reglas del repo que aplican siempre
- El **host** (Rust 2024) es dueño de la lógica; los clientes solo muestran y mandan intenciones. Si un cliente necesita algo,
  se añade un método al host.
- Rust: sin `unwrap()` fuera de pruebas, enums en vez de cadenas para estados, `tracing` con campos, trabajo bloqueante en
  `spawn_blocking`, ningún archivo nuevo de más de ~500 líneas.
- **Todo cambio de interfaz va a los cuatro clientes** (escritorio, Android, iOS, terminal `host/src/tui/`). Si un cliente no
  aplica, se documenta el motivo.
- Pruebas de puerta por capa: host `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test -j 1 -- --test-threads=1`;
  escritorio `npm run typecheck` y `npm test`; Android `./gradlew.bat assembleDebug testDebugUnitTest :core:test lintDebug`.
  iOS no compila en esta PC: se escribe con cuidado y se marca «no compilado».
- **Prueba de extremo a extremo real** del flujo cambiado (host de desarrollo en el puerto 19223, no el de producción 19222).

### 0.3 La máquina (8 GB de RAM)
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1`; el host entero compila en ~20 min. Gradle: `--no-daemon -Dorg.gradle.jvmargs=-Xmx2g`.
  No compilar Rust y Gradle a la vez; antes de Rust, `./gradlew.bat --stop`.
- Comprobar >20 GB libres en C:; si baja de 8 GB, borrar solo `host/target/debug/incremental`.
- electron-builder: generar antes `apps/desktop/dist/icon.ico` y usar `-c.win.icon=dist/icon.ico`.
- APK de Android: firmar con `~\.codync-signing\codync-release.jks` (alias `codync`), nunca con la clave de depuración. Instalar por USB
  exige aceptar el aviso de MIUI.

### 0.4 Usar código de los repos de referencia
- Copias en `C:\Users\Edwin Villeda\Documents\ref-repos\` (solo lectura): `bdots`, `OpenDots`, `rakazo`, `coucou`, `mercury-agent`.
- Licencias: MIT (Coworkers, OpenDots, Coucou, Mercury) y Apache-2.0 (Rakazo). Al copiar código: dejar el aviso de copyright del
  archivo, añadir el origen a un archivo de atribuciones de terceros y, con Apache-2.0, conservar el `NOTICE` y marcar el archivo
  como modificado. El nombre, Mochi, el arte y los sonidos de Coucou no se reutilizan.
- Regla práctica: **el host se porta a Rust (diseño y pruebas), el escritorio puede copiar TypeScript, Android y iOS portan el
  diseño.**

### 0.5 Plantilla de cada paquete
Cada PT termina con: commits pequeños, documento nuevo o actualizado en `docs/features/`, informe breve con la salida de cada
puerta y una lista de lo no probado. Se sube la rama a `origin` al cerrar cada fase.

---

## PT1. Aprobaciones completas
**Objetivo:** que ninguna aprobación se quede colgada, que las reglas se puedan editar y que «Siempre» cree una regla.

**Estado actual:** `host/src/agent/bot/updates.rs` (`on_permission`, `answer_permission`, `expire_abandoned`), `host/src/agent/approval_rules.rs`,
`respondPermission` en `host/src/api/bots.rs`; `docs/features/approvals.md` dice que «ningún cliente edita las reglas todavía».
`actionRules` (siete clases) se guardan en `host/src/store/model.rs` y nadie las aplica. Alertas: `AlertKind { Done, NeedsInput, Failed }`
en `host/src/remote/push.rs`.

**Referencia:** `rakazo/packages/core/src/stuck-work.ts` y su prueba (4 h aviso, 24 h cierre, 30 min para tomas de control);
`mercury-agent/src/capabilities/shell/blocklist.ts`; `coucou/NotchBuddy/Sources/App/PhoneLink/ApprovalRelay.swift` (huella y regla de no
permitir «siempre» desde el teléfono).

### PT1a. Aviso a las 4 h y cierre a las 24 h
1. Crear `host/src/agent/bot/stuck.rs` con una función pura `decide(status, esperando_desde, ahora) -> Acción { Ninguna, Avisar, Cerrar }`
   y las constantes de Rakazo (`NOTIFY_AFTER = 4 h`, `EXPIRE_AFTER = 24 h`, `TAKEOVER_EXPIRE_AFTER = 30 min`). Estados: tarjeta pendiente,
   a la espera de respuesta y a la espera de toma de control.
2. Portar los casos de `stuck-work.test.ts` a pruebas Rust (reloj inyectado, como hace `proactive::decide`).
3. Un barredor periódico arrancado desde `host/src/background.rs` (mismo patrón que `proactive::run`) que recorre las tarjetas pendientes.
4. «Avisar» marca la entrada con una bandera para que el aviso salga **una sola vez por episodio** y envía un push con `AlertKind::NeedsInput`.
5. «Cerrar» reutiliza `expire_abandoned` (la tarjeta pasa a `expired` con sus botones) y deja un aviso en el chat.
6. La toma de control (`host/src/chat/handoff.rs`) caduca a los 30 min porque retiene la computadora del bot.
7. **Clientes:** el estado `expired` ya existe en las tarjetas; comprobar que los cuatro lo muestren igual.
- **Pruebas:** unitarias de `decide`; integración con el agente falso (`host/tests/e2e.rs` con `fake_agent.py`) creando una aprobación y adelantando el reloj.
- **Aceptación:** una aprobación sin respuesta genera un solo aviso a las 4 h y se cierra a las 24 h; una toma de control sin atender se libera a los 30 min.

### PT1b. Editor de reglas de aprobación en los clientes
1. Mantener el contrato: `{pattern, kind?, decision}`, 100 reglas como máximo, 500 caracteres cada una (`approvals.md`).
2. Pantalla en la ficha del bot (escritorio `BotEditorView.tsx`, Android `BotEditor.kt`, iOS y TUI): lista ordenada (el primer acierto decide),
   añadir, editar, borrar, reordenar.
3. **Probador de reglas:** campo «escribe un comando» que muestra qué regla lo decide. Para que el cliente no copie lógica, añadir al host un método
   `testApprovalRule {botId, command, kind?}` que usa el mismo código que `approval_rules.rs`.
4. Mostrar la advertencia del documento: un `allow` nunca aprueba comandos encadenados (`;`, `&&`, `||`, `|`, `` ` ``, `$(`, `>`, `<`, salto de línea).
- **Aceptación:** crear, probar y borrar una regla en los cuatro clientes cambia el comportamiento de una aprobación real.

### PT1c. «Siempre» como regla guardada
1. Cuando se elige `allow_always` en una tarjeta, el host propone una regla (`pattern` = el comando o el título, `kind` = el tipo de herramienta).
2. **Solo desde la computadora.** Desde el teléfono, «Siempre» no existe (regla de Coucou); mostrar solo Permitir y Denegar. Documentarlo.
3. Pedir confirmación mostrando el patrón antes de guardarlo y permitir editarlo; nunca guardar patrones que contengan encadenamientos.
- **Aceptación:** tras «Siempre», el mismo comando no vuelve a preguntar; la regla aparece en PT1b y se puede borrar.

### PT1d. Decidir qué hacer con `actionRules`
Hay dos sistemas a medio hacer (clases de acción y reglas por patrón). Recomendación: **compilar las clases a reglas** al leerlas (por ejemplo
`shell: ask` → reglas `kind=execute, decision=ask`), de modo que la evaluación siga siendo una sola. Pasos: mapa clase→tipos de herramienta, prueba por clase y
una sola pantalla. Si prefieres quitar `actionRules`, borrar el campo y su prueba en `store/model.rs`.

### PT1e. Lista fija de comandos prohibidos
1. Portar `blocklist.ts` a una constante Rust (patrones glob; incluye variantes de PowerShell para borrar la raíz o el home). Atribuir a Mercury (MIT).
2. Evaluarla **antes** de las reglas y del modo «auto»: coincide → rechazar con aviso que nombra el patrón.
3. Reutilizar el emparejador de `approval_rules.rs` para el glob; no escribir otro.
- **Pruebas:** casos del propio `blocklist.ts` más variantes de Windows.
- **Aceptación:** con un bot en «auto», `rm -rf /` y `Remove-Item -Recurse -Force C:\` se rechazan.

---

## PT2. Ahorro de tokens, modelo de fondo y roles de modelo
**Objetivo:** que los turnos sin nadie delante (rutinas y check-ins) usen un modelo más barato y que el presupuesto actúe solo.

**Estado actual:** `host/src/budget.rs` (aviso al 80 %, pausa de rutinas al 100 %). Las rutinas corren en una sesión propia; los check-ins
(`host/src/proactive.rs`) usan `dispatch_bot_turn` sobre la sesión principal. `host/src/chat/oneshot.rs` corre trabajos de fondo en un agente descartable.

**Referencia:** `bdots/src/agents/bot-agent.ts` (cerca de la línea 943, `BACKGROUND_MODEL` y el pie de turno de fondo en `prompt.ts`), `mercury-agent/src/core/saver-mode.ts`,
`rakazo/packages/core/src/model-role-settings.ts`.

Pasos:
1. Añadir a `BotConfig` (`host/src/store/model.rs`) `background_model: Option<String>` y `saver: bool`, con valores por defecto en la deserialización
   (hay una prueba que ya garantiza los valores por defecto de `proactive` y `actionRules`; copiar ese patrón).
2. **Verificar** cómo se elige el modelo por bot (`agentModels`, `AgentModelPicker`). Si el modelo se fija al crear la sesión, los check-ins deben
   ejecutarse por el camino de las rutinas (sesión propia) para poder usar otro modelo; si no, solo cambia el texto.
3. Unificar el texto del turno de fondo («el usuario no mira; trabaja por tu cuenta») en un solo lugar y usarlo en `proactive.rs` y en las rutinas.
4. **Modo ahorro:** al llegar al 80 % de `budget.rs`, activar el ahorro (modelo de fondo, menos contexto) y avisar; desactivarlo al día siguiente.
5. Clientes: selector «Modelo de fondo» y un interruptor «Ahorro» en la ficha del bot, junto al presupuesto.
- **Pruebas:** unitarias de la activación por umbral; e2e con dos modelos falsos comprobando cuál recibe cada turno.
- **Aceptación:** una rutina corre con el modelo de fondo y el chat normal con el principal; al 80 % se activa el ahorro.

---

## PT3. Fin de turno honesto y «hecho» con evidencia
**Objetivo:** distinguir «terminó» de «se quedó sin pasos o se cortó» y, opcionalmente, exigir una verificación real.

**Estado actual:** el host ya guarda el motivo de parada en `turnResult.stopReason` (`host/src/agent/bot/results.rs`, `turn.rs:310`). Falta traducirlo a estados y avisos.

**Referencia:** `mercury-agent/src/core/completion-verdict.ts` (qué cuenta como éxito y qué como pausa), `execute-guard.ts`, `programming-mode.ts`. Su propio
`ROADMAP.md` admite que la puerta de verificación fallaba por comparar texto: hazla con el **código de salida**.

Pasos:
1. Enumerar los `stopReason` que llegan por ACP (`end_turn`, agotamiento de pasos o de tokens, `cancelled`, errores) y mapearlos a un enum `TurnVerdict { Completed, Paused, Truncated, Cancelled, Failed }`.
2. `Paused` y `Truncated` no disparan «terminó»: estado visible «pausado a la mitad», notificación distinta y un botón **Continuar**.
3. **Puerta de verificación (opcional por bot):** campo `verify_command` (por ejemplo `npm test`). Al terminar un turno de código, el host lo ejecuta en la carpeta del bot con
   `spawn_blocking` y tiempo máximo; código 0 = «hecho»; distinto de 0 = aviso con la salida resumida y oferta de seguir.
4. Cuatro clientes: estado nuevo en la lista, tarjeta de pausa y ajuste de verificación.
- **Aceptación:** un turno cortado por pasos muestra «pausado» y no «terminó»; con verificación activa, un test rojo impide marcar «hecho».

---

## PT4. Perfiles de seguridad al crear un bot
**Objetivo:** una sola pregunta al crear el bot que configure de verdad permisos y límites.

**Referencia:** `mercury-agent/src/bots/permission-tiers.ts` (solo lectura, constructor, operador y total).

Pasos:
1. Definir en el host un enum `SecurityProfile { ReadOnly, Builder, Operator, Full }` y una función que lo traduce a `permission`, `approvalRules` y presupuesto.
   Los alcances de carpeta de Mercury no aplican a agentes externos por ACP; aplican a lo que Codync maneja directamente.
2. Aplicar PT1e (lista prohibida) a todos los perfiles, incluido «Full».
3. Pantalla de creación de bot (`NewBotView`/`NewGroupView` en Android; equivalentes en escritorio e iOS): selector con una frase por perfil y vista previa de lo que activa.
4. Mostrar el perfil en la ficha y permitir cambiarlo (con aviso de que sobrescribe reglas).
- **Aceptación:** crear un bot «Solo lectura» pregunta antes de editar archivos y niega comandos; «Full» sigue respetando la lista prohibida.

---

## PT5. Preguntas y planes de sesiones externas (hooks)
**Objetivo:** que las preguntas (`AskUserQuestion`) y los planes de Claude Code lleguen a cualquier cliente y la respuesta vuelva a la terminal.

**Estado actual:** `host/src/external/permission.rs` devuelve `None` para `AskUserQuestion` y deja que la terminal pregunte (línea ~68). La instalación (`install.rs`)
escribe solo el hook de Claude Code.

**Referencia:** `coucou/NotchBuddy/Sources/App/HookServer.swift` (usa un **hook `PreToolUse` dedicado** para `AskUserQuestion` que mantiene la conexión abierta mientras el usuario responde),
`QuestionPayload.swift` (hasta 4 preguntas, selección única o múltiple, y solo se aceptan etiquetas que estén entre las opciones vigentes), `PhoneLink/QuestionRelay.swift`
(respuesta de un solo uso, se borra al leerla), `windows/hook/src/normalize.rs` (Rust, MIT).

Pasos:
1. Instalar un segundo hook `PreToolUse` con filtro `AskUserQuestion` en `install.rs`, con vista previa y respaldo del `settings.json` (reglas de `ClaudeSettingsFile.swift`).
2. En `permission.rs`, en vez de devolver `None`, crear una entrada de tarjeta con las preguntas y opciones. **Verificar** si conviene reutilizar `EntryKind::Permission` con `options`
   o añadir un tipo nuevo; reutilizar es más barato porque los cuatro clientes ya dibujan tarjetas con opciones.
3. Validar en el host que la respuesta sea una de las opciones vigentes y que corresponda a la pregunta pendiente (huella: sesión, herramienta e identificador de la pregunta).
4. Devolver la elección por el canal del hook. **Verificar** en la documentación de hooks de Claude Code qué formato de respuesta admite para `AskUserQuestion`; si no existe,
   usar el mismo mecanismo que Coucou.
5. Tiempo límite: si nadie responde, devolver «sin opinión» para que la terminal pregunte, como ya hace `request()`.
6. Mismo camino para la aprobación de un plan (`ExitPlanMode`).
- **Pruebas:** unitarias de la validación de etiquetas; e2e con un hook simulado.
- **Aceptación:** una pregunta de Claude Code aparece como tarjeta en el teléfono; elegir una opción la contesta en la terminal; una respuesta fuera de las opciones se rechaza.

---

## PT6. Canales de mensajería: Telegram y Slack sin endpoint público
**Objetivo:** hablar con tus bots desde Telegram y quitar el requisito de una URL HTTPS pública en Slack.

**Estado actual:** `host/src/slack/` (`ingress.rs`, `mod.rs`, `worker.rs`), `host/src/store/slack.rs`, `docs/features/slack-channel.md`, `packaging/slack-app-manifest.json`.
Implementado en código; **sin probar con un workspace real**. `tokio-tungstenite` y `reqwest` ya son dependencias.

**Referencia:** `mercury-agent/src/channels/` (Telegram con botones y emparejamiento por código, Slack Socket Mode),
`rakazo/packages/core/src/messaging-providers.ts` y `messaging-commands.ts` (varios bots de Telegram con nombre, comandos).

Pasos:
1. **Probar Slack primero** con un workspace real y corregir lo que falle. No construir más canales sobre algo no probado.
2. Extraer un contrato de canal de `slack/`: normalizar un mensaje entrante a `{canal, persona, hilo, texto, archivos}`, enviar respuestas y enviar tarjetas de aprobación.
   Mover lo común a `host/src/channels/` y dejar lo propio de Slack en `slack/`.
3. **Slack Socket Mode:** `apps.connections.open` devuelve una URL `wss`; conectar con `tokio-tungstenite`, confirmar cada evento y reconectar con espera creciente.
   Elimina el endpoint público.
4. **Telegram:** token del bot en la bóveda (`market/vault.rs`); recepción por `getUpdates` (conexión saliente, sin endpoint); emparejamiento: el bot responde con un código
   y la persona lo escribe en Codync para vincular su chat; lista de permitidos por persona; respuestas con `editMessageText` mientras llega el streaming; aprobaciones con botones
   en línea (`callback_query`) que llaman a `respondPermission` con la huella de la tarjeta; archivos con `sendDocument`.
5. Todas las llamadas salientes por el cliente con política de red de `host/src/egress.rs` (Telegram y Slack en la lista de permitidos).
6. Comandos de chat sin gastar modelo: `/estado`, `/rutinas`, `/detener` (Rakazo).
7. Clientes: pantalla «Canales» con alta, emparejamiento, lista de permitidos y estado.
- **Pruebas:** servidor falso de Telegram y de Slack en las pruebas del host; casos de emparejamiento y de aprobación duplicada (la primera respuesta gana).
- **Aceptación:** desde Telegram se escribe a un bot, se aprueba un permiso con un botón y se reciben archivos.
- **Riesgos:** exponer comandos a personas no autorizadas (por eso la lista de permitidos), y filtrar datos en chats de grupo.

---

## PT7. Importar agentes de otras herramientas
**Objetivo:** «Importar un agente» desde Hermes u OpenClaw con instrucciones, memoria y habilidades, sin copiar credenciales.

**Referencia:** `rakazo/packages/core/src/agent-import/` (14 archivos: `hermes.ts`, `openclaw.ts`, `config.ts`, `credentials.ts`, `archive.ts`, `document.ts`, `shared.ts`, y sus pruebas).
Apache-2.0: conservar el `NOTICE` y marcar los archivos modificados.

Pasos:
1. Copiar los lectores a `apps/desktop/src/main/agent-import/` (mismo lenguaje, sin reescribir) con sus pruebas, y respetar sus límites de archivos y tamaño.
2. Mantener la regla de Rakazo: **detectar credenciales y no importarlas**; mostrarlas en la vista previa como «omitidas».
3. Vista previa antes de crear: instrucciones, archivos de memoria y habilidades detectadas, con casillas para elegir.
4. Crear el bot por la API del host (`createBot`/`updateBot`) y escribir la memoria con los métodos de memoria; las habilidades por `chat/skills.rs`.
5. Cliente: botón «Importar» en el escritorio (el único que lee carpetas locales); Android e iOS pueden iniciar la importación contra un host de escritorio.
- **Aceptación:** importar una carpeta de prueba de cada formato crea un bot con instrucciones y memoria, y ninguna credencial llega a la base.

---

## PT8. «Tu semana» y modo demo
**Objetivo:** una tarjeta semanal con el resumen del trabajo de los bots y un recorrido sin agente ni red.

**Referencia:** `coucou/NotchBuddy/Sources/App/RecapStore.swift` (turno, archivos, líneas añadidas y quitadas, comandos, preguntas y decisiones),
`DemoEngine.swift` (recorrido de ~60 s «sin red ni disco»).

### Resumen semanal
1. `host/src/recap.rs`: consulta SQLite de la semana (turnos, herramientas con diff, aprobaciones respondidas, rutinas ejecutadas y falladas) y calcula las cifras; sin modelo ni red.
2. Programarlo desde `background.rs` para el lunes por la mañana en la zona horaria del bot (el patrón de `proactive.rs` ya resuelve zonas); guardar la instantánea en `kv`.
3. Método `weeklyRecap` y evento para los clientes; tarjeta en el chat principal y en el tablero de estado. Con un modelo opcional para redactar la frase de apertura.
### Modo demo
1. Un interruptor «Probar modo demo» por cliente que sustituye el almacén de datos por un guion en memoria: bots de ejemplo, una sesión con pasos y un diff, una aprobación.
2. Garantía visible: no hay llamadas de red ni escritura en disco ni en el llavero mientras dura.
- **Aceptación:** el modo demo corre sin host ni cuenta; el resumen de una semana con datos de prueba coincide con las cifras esperadas.

---

## PT9. Panel de salud de la computadora y gobernador de procesos
**Objetivo:** mostrar RAM, CPU y disco, avisar antes de ahogar la PC y recortar el consumo de los agentes.

**Estado actual:** sin medición de memoria de procesos. `sysinfo` **no** es dependencia hoy. `host/src/agent/idle.rs` ya cierra sesiones ociosas.

**Referencia:** `mercury-agent/src/core/memory-governor.ts` (respuesta graduada comprobada entre pasos), `coucou/NotchBuddy/Sources/App/MachineVitals.swift`.

Pasos:
1. Añadir la dependencia `sysinfo` (MIT) y un módulo `host/src/health.rs` que mida RAM total y libre, CPU y disco, y el consumo de cada proceso de agente que Codync lanzó (el host conoce sus PID).
2. Política graduada con umbrales configurables: bajo 70 % normal; 70-85 % aviso en el chat y en la isla; más de 85 % no lanzar turnos nuevos y cerrar sesiones ociosas (`idle.rs`);
   un proceso que crece de forma anormal se avisa y, si persiste, se detiene con aviso.
3. Método `hostHealth` y evento; pantalla en la pestaña de estado y en la isla; en la terminal, una línea de estado.
4. **Aviso previo:** antes de lanzar un turno costoso, si la memoria libre es baja, preguntar.
- **Pruebas:** unitarias de la política con lecturas simuladas.
- **Aceptación:** con memoria simulada al 90 %, no se inicia un turno nuevo y se informa por qué.

---

## PT10. «Lo que sé de ti»: memoria como creencias
**Objetivo:** confirmar, corregir y olvidar lo que cada bot sabe de ti, con avisos breves de lo aprendido.

**Estado actual:** la memoria son archivos (`MEMORY.md` por capa) con versiones recientes y «dreaming» (`host/src/chat/memory/`, `docs/design/bot-memory.md`).

**Referencia:** `mercury-agent/src/learning/announce.ts` (una línea por turno como máximo; una pregunta de contradicción como máximo al día), `system/confirm-belief.ts`,
`correct-belief.ts`, `forget-memory.ts`.

Pasos:
1. Añadir metadatos a cada nota (tipo, confianza, última confirmación) en un bloque de cabecera del archivo, de modo que siga siendo texto editable.
2. Herramientas del bot en el servidor de memoria: confirmar, corregir y olvidar (con versión nueva, no borrado definitivo).
3. Regla de aviso: tras un cambio que modifica el comportamiento, una línea «aprendí X» con **Deshacer**; una pregunta de contradicción como máximo cada 24 h.
4. Pantalla «Lo que sé de ti» sobre la lista de notas que ya expone `file_api.rs`, con filtros por tipo y confianza.
- **Aceptación:** una nota se confirma, se corrige y se olvida desde la pantalla, y el cambio aparece en el historial de versiones.

---

## PT11. Bandeja de habilidades propuestas
**Objetivo:** que el bot proponga guardar un procedimiento como habilidad y que tú lo apruebes.

**Referencia:** `mercury-agent/src/bots/skill-synthesis.ts` (si una ejecución usó 3 o más herramientas distintas, vale la pena guardarla), `OpenDots/src/server/learning.ts`.
**Estado actual:** `host/src/chat/skills.rs`, `teach.rs` y `host/src/chat/oneshot.rs` (un trabajo de fondo en un agente descartable, usado para el «dreaming» y el nombre automático).

Pasos:
1. Al terminar una rutina o un turno, contar herramientas distintas usadas; si son 3 o más y el turno tuvo éxito, encolar una propuesta.
2. Redactar el borrador con `oneshot.rs` (nombre, cuándo usarla, pasos) y guardarlo en una tabla de propuestas **pendientes**, nunca como habilidad activa.
3. Métodos para listar, editar, aprobar y descartar; aprobar guarda con `skills.rs`.
4. Clientes: bandeja con una tarjeta por propuesta y un contador en la ficha del bot.
- **Aceptación:** tras una rutina compleja aparece una propuesta; aprobada, el bot la ve como habilidad; descartada, no vuelve a salir igual.

---

## PT12. Paquetes grandes (esquema, no paso a paso)
Estos requieren un documento de diseño antes de escribir código.

| PT | Qué | Primer paso | Referencia |
|---|---|---|---|
| PT12a | **Tablero Kanban** | Modelo de «tarjeta» sobre datos que ya existen (rutinas, elementos abiertos, aprobaciones, tareas de equipos) y estados en columnas; evento del hub por cada cambio | `mercury-agent/src/core/task-board.ts`, `board-manager.ts`, `board-db.ts` |
| PT12b | **Cabina web** | Decidir cómo hace el navegador el cifrado extremo a extremo (WebCrypto) contra el relay; empezar con lista de bots, chat y aprobaciones | `mercury-agent/ui/`, `rakazo/apps/web` |
| PT12c | **Gráficas en el chat** | Esquema JSON validado de una gráfica; el escritorio elige una librería (justificarlo por escrito); Android con Canvas; iOS con Swift Charts; ayudante para mini-apps | `rakazo/packages/core/src/plot/plot-spec.ts` |
| PT12d | **Tableros de servicios** | `host/src/services/github.rs`: sondeo con token de la bóveda, instantánea y detalle; empezar solo con GitHub (PRs y CI) | `coucou/.../GithubPoller.swift`, `ServicePublisher.swift` |
| PT12e | **Estudio de dictado global** | Dictado a nivel de sistema con una tecla mantenida, diccionario y limpieza con modelo pequeño y plazo de 4 s; elegir un motor fuera de línea para Windows | `coucou/.../FlowDictation.swift`, `FlowCleanup.swift` |
| PT12f | **Importar tuberías y rutinas por nodos** | Ya hay prompts propios para el motor y las pantallas; añadir el paso «aprobación humana» | `bdots/src/agents/pipelines.ts` |
| PT12g | **Notas de reuniones** | Captura de audio del sistema en Windows (WASAPI loopback) y transcripción; es un proyecto en sí mismo | `coucou/.../NoteTaker.swift` |

---

## 1. Orden recomendado

| Orden | PT | Tamaño | Depende de |
|---|---|---|---|
| 1 | PT1a, PT1e | S-M | nada |
| 2 | PT5 | M-L | nada |
| 3 | PT6 (primero probar Slack) | L | nada |
| 4 | PT3 | M-L | nada |
| 5 | PT1b, PT1c, PT1d, PT4 | M | PT1e |
| 6 | PT2 | M | nada |
| 7 | PT9 | M-L | nada |
| 8 | PT7, PT8 | M-L | nada |
| 9 | PT10, PT11 | L | nada |
| 10 | PT12 | L-XL | cada uno con su diseño |

## 2. Lista de comprobación por paquete

- [ ] Rama propia desde la base correcta; sin tocar otros worktrees.
- [ ] Host: `fmt`, `clippy -D warnings` y pruebas en verde.
- [ ] Los cuatro clientes cubiertos (o el motivo documentado).
- [ ] Prueba de extremo a extremo real, no solo pruebas unitarias.
- [ ] Atribución de terceros si se copió código; `NOTICE` si es Apache-2.0.
- [ ] `docs/features/` actualizado en el mismo cambio.
- [ ] Rama subida a `origin` sin `--force`; sin subir versión.
- [ ] Informe con lo no probado (por ejemplo iOS no compilado).
