# Ideas parciales y nuevas por repo: cómo integrarlas y si conviene usar su código (2026-10-28)

Complementa `analisis-ideas-6-repos-2026-10-28.md`. Aquí cada repo tiene su lista de ideas con el cómo en Codync y un veredicto sobre reutilizar o no su código.

**Esta versión solo conserva lo que Codync tiene a medias (🟡) o no tiene (❌).** Se quitaron 40 ideas que ya existen
(por ejemplo el editor TipTap, los conectores de Composio por bot, el vigilante de atascos, la cola de mensajes y las pruebas
e2e con agente falso) y se verificaron las que antes figuraban como «no comprobadas». Las ideas grandes de hooks y
CloudKit están en `ideas-nuevas-y-grandes-2026-10-28.md`.

## 0. Qué cambió respecto al análisis anterior

- **Esta vez leí el código, no solo el README.** Cloné los cinco repos (sin historial, solo lectura) en
  `C:\Users\Edwin Villeda\Documents\ref-repos\` el 2026-10-28:
  `bdots` (189 archivos), `OpenDots` (206), `rakazo` (3 638), `coucou` (1 167) y `mercury-agent` (1 416).
  Además tienes Openstark en `Documents\Openstark`. No escribí en ninguno.
- **Corrección de licencias.** Mi análisis anterior decía que GitHub no detectaba licencia y que de
  Coworkers no constaba ninguna. Los cinco tienen archivo `LICENSE`: **MIT** (Coworkers/bdots, OpenDots,
  Coucou, Mercury) y **Apache-2.0** (Rakazo). El código se puede reutilizar con sus avisos; lo único
  reservado son los nombres y artes de Coucou (`LICENSE-ASSETS.md`). Corregí también el documento anterior.
- Son **cinco repos distintos**, no seis: `fayazara/bdots` y `fayazara/coworkers` son el mismo.
- **«El código de referencia que ya tenemos»:** lo interpreto como estas copias locales y lo que Codync ya
  tiene adoptado (`budget.rs`, `egress.rs` y los demás de la sección 4 del análisis anterior).

## 1. Cómo leer las tablas

**En Codync hoy:** 🟡 parcial · ❌ no lo encontré (con `git grep` en el host, escritorio, Android, iOS y `docs/features`).
Las que ya existen (✅) se quitaron de las tablas.

**Veredicto sobre el código de referencia:**

| Clave | Significa | Cuándo |
|---|---|---|
| **A** | Reutilizar copiando y adaptando | Mismo lenguaje y capa que donde iría (por ejemplo TypeScript para el escritorio o un script de navegador) |
| **B** | Portar el diseño, las constantes y las pruebas; reescribir | Otro lenguaje (casi todo el host de Codync es Rust) o depende de su arquitectura |
| **C** | Solo la idea | Depende de su infraestructura: Cloudflare, Postgres, CopilotKit Intelligence, Docker |
| **D** | No conviene | Choca con una regla de Codync o con licencias de arte |

**Esfuerzo:** (S) pocos días · (M) una o dos semanas · (L) más de dos semanas.

**Reglas de uso de código ajeno:**
1. MIT: conservar el aviso de copyright y la licencia del archivo copiado. Apache-2.0 (Rakazo): además
   conservar un archivo de avisos (`NOTICE`) y marcar que el archivo fue modificado.
2. Añadir cada origen a un archivo de atribuciones de terceros del repo y dejar un comentario en la cabecera
   del archivo nuevo, como ya hacen `budget.rs` y `egress.rs`.
3. Los nombres, Mochi y los sonidos de Coucou no se reutilizan.
4. `CLAUDE.md` dice «no reintroducir hooks ni CloudKit», pero Codync ya tiene hooks para sesiones externas
   (`host/src/external/`). Al ser un proyecto personal, los hooks y CloudKit se tratan como ideas, no como prohibiciones.

## 2. Veredicto global por repo

| Repo | Pila | Licencia | ¿Vale la pena su código? |
|---|---|---|---|
| **Coworkers (bdots)** | TypeScript sobre Cloudflare Workers; un agente de 5 107 líneas | MIT | **Parcial.** Reutilizar tal cual los trozos puros: el script de grabación de demostraciones (`teach.ts`, JS que corre en el navegador del bot), las 32 recetas de transiciones de `.agents/skills/transitions-dev` y las pruebas de `guards`, `pipelines` y `secrets`. Portar a Rust la lógica. No usar `bot-agent.ts` ni `user-hub.ts` |
| **OpenDots** | TypeScript, CopilotKit | MIT | **Poco.** Depende del servicio CopilotKit Intelligence. Sirve el editor de páginas (TipTap con comandos «/» y autoguardado) si el escritorio lo necesita, y las reglas del ejecutor con arrendamiento. El resto, solo ideas |
| **Rakazo** | Monorepo TypeScript, Postgres, Expo | Apache-2.0 | **Sí, en piezas puras.** `stuck-work.ts`, `unread-divider.ts`, `time-separators.ts`, `text-direction.ts`, `plot-spec.ts`, `speech-text.ts` y los lectores de `agent-import/` se pueden copiar al escritorio o a su proceso principal. El resto del núcleo asume Postgres |
| **Coucou** | Swift (Mac/iOS), Kotlin (Android), Rust/Tauri (Windows) | MIT; arte reservado | **Parcial.** Los widgets de Android usan `RemoteViews` clásicos, no Glance como Codync, así que se porta la idea y se pueden reaprovechar los 8 drawables por estado y la lógica de `WidgetData.kt` y `WidgetSpoken.kt`. De Swift, solo las piezas puras para iOS. Los hooks ya existen en Codync (`host/src/external/`, diseñados a partir de Coucou) y CloudKit queda como idea |
| **Mercury** | TypeScript, un agente de 8 362 líneas | MIT | **Sí, en tres cosas:** la lista de comandos prohibidos del shell (`blocklist.ts`), los módulos puros (`completion-verdict`, `stall-watchdog`, `permission-tiers`) como especificación con pruebas, y los acompañantes en **Rust** (`plugins/whatsapp/companion`, `plugins/optimus/companion`), que son del mismo lenguaje que el host |

---

## 3. Coworkers / bdots (22 ideas)

| # | Idea (dónde está) | Cómo integrarla en Codync | Hoy | Código |
|---|---|---|---|---|
| 1 | **Guardia anti-repetición:** bloquea tras 5 llamadas idénticas seguidas, comparando JSON con claves ordenadas (`agents/guards.ts`, 183 l) | Ampliar `host/src/agent/bot/stall.rs` para contar llamadas idénticas que sí tienen éxito, no solo las que fallan. Mensaje al agente pidiendo parar (S) | 🟡 `stall.rs` cubre fallos iguales | B, con sus pruebas |
| 2 | **Aprobación sin respuesta:** push a las 4 h y cierre a las 24 h (README; el mismo valor en Rakazo) | Planificador en el host sobre las tarjetas `pending`, evento push nuevo y cierre vía `expire_abandoned` (S-M) | ❌ | B |
| 3 | **Pipelines de hasta 10 pasos** con pasos de bot y **pasos de aprobación humana** intercalados; el primero tiene que ser un bot (`agents/pipelines.ts`, 87 l) | Es el modelo de las rutinas por nodos con un nodo «aprobación». Validación idéntica en el host (M-L) | ❌ | B. La validación se copia como especificación |
| 4 | **Editor y ejecuciones de pipeline** (`pipeline-editor.tsx`, `pipeline-runs.tsx`) | Pantalla de pasos y de historial por ejecución en los cuatro clientes (L) | ❌ | C |
| 5 | **Secretos:** tarjeta enmascarada; el valor queda atado a un origen HTTPS exacto; el bot pide la petición sin ver el valor; esquemas bearer, basic o cabecera (`agents/secrets.ts`) | Herramienta MCP `secret_request` en el host + tarjeta en cada cliente; guardar en `market/vault.rs`; el host hace la petición HTTP (M) | 🟡 Hay bóveda y `type_login`; falta la petición HTTP con secreto atado a origen | B |
| 6 | **Redacción del secreto en la respuesta**, en todas sus formas (valor y `encodeURIComponent`) (`redact`) | Pasar toda respuesta de la herramienta anterior por un filtro antes de dársela al bot (S) | ❌ No encontré redacción de secretos en las respuestas | B |
| 7 | **Grabador de demostraciones:** un script de página registra clics, cambios y envíos en `window.name` y `localStorage`; ignora campos tipo `pass | otp | cvv | card | token`; el historial de navegación lo da CDP (`agents/teach.ts`, 177 l) | Inyectarlo con CDP en `host/src/browser/`; el host lo convierte en habilidad con `chat/teach.rs` (M) | 🟡 `teach.rs` existe; no revisé qué graba | **A** para el script (JS puro); B para el resto |
| 8 | `start_teaching`, `finish_teaching`, `save_skill` | Verificar que `chat/skills.rs` y `teach.rs` expongan estas tres herramientas (S) | 🟡 | B |
| 10 | **Turnos de fondo con modelo más barato** (`BACKGROUND_MODEL`, `bot-agent.ts:943`) | Ajuste por bot «modelo de fondo» para rutinas y check-ins, pasado en `session/new` del ACP (S-M) | ❌ | B |
| 11 | **Pie de turno de fondo:** «el usuario no mira» + reglas de «trabajar por tu cuenta» (`agents/prompt.ts`, 579 l) | Unificar el texto de `proactive.rs` y de las rutinas en un solo prompt de segundo plano (S) | 🟡 | B. Leerlo y adaptarlo |
| 12 | **Correo entrante** con dirección `usuario.bot@dominio`; guarda el mensaje en `/workspace/inbox/email/<fecha>-<asunto>` e ignora autorrespuestas (`onEmail`) | Dirección por bot en el relay de Codync Cloud y entrega al host como evento `email` (L, requiere dominio) | 🟡 El evento `email` de rutinas existe | C |
| 20 | **`ask_user`:** el bot hace preguntas con opciones | Herramienta MCP + tarjeta con opciones (única o múltiple) en los cuatro clientes (M) | ❌ Solo `external/permission.rs` menciona `AskUserQuestion`, y lo omite | B |
| 21 | **Conectar una app una vez en el hub y que la usen todos los bots** (`agents/user-hub.ts`) | Ya hay «Connectors» por bot; falta una conexión a nivel de usuario que cada bot active (S-M) | 🟡 | C |
| 22 | **Reglas por tipo de acción** con permitir/preguntar/bloquear | Aplicar `actionRules` o fusionarlas con `approvalRules` (M) | 🟡 | B |
| 23 | **Defecto seguro sin nadie delante:** en segundo plano solo herramientas de apps de solo lectura | Si el turno es de fondo, denegar herramientas MCP sin `readOnlyHint` (S) | ❌ | B |
| 24 | **Vista en vivo del navegador** (`agents/browser-frame.ts`, 93 l) | Ya existe el panel de computadora; revisar el protocolo de cuadros para reducir latencia (M) | 🟡 | C |
| 25 | **Scripts de JavaScript con pausa para tu visto bueno** (`run_js` + `script_wait`) | Choca con el modelo de Codync (los agentes ya tienen shell). Solo útil como «tarea con pausa» dentro de rutinas por nodos | ❌ | D |
| 27 | **Agrupar los pasos de trabajo** en un bloque colapsable «trabajó 12 pasos» (`work-group.tsx`) | Revisar cómo agrupan `ChatRows` (escritorio) y `ChatRows.kt`; añadir el resumen si falta (S) | ❌ No encontré agrupación de pasos | A (TS) si el escritorio no lo tiene |
| 28 | **Biblioteca de 32 transiciones** (`.agents/skills/transitions-dev`: toast, contador giratorio, shimmer, estados de «pensando», texto en streaming, apilado de banners…) | Documento de movimiento para los clientes; Codync ya usa `SpinningCounter` (escritorio) (S) | 🟡 | **A** (son Markdown y CSS, MIT) |
| 29 | **Skills de diseño** (`make-interfaces-feel-better`, `emil-design-eng`) | Añadirlos a `.agents/skills` del repo para las revisiones de UI (S) | ❌ | **A** |
| 30 | **Instalador web con OAuth que despliega todo** (`installer/`) | Idea para desplegar un relay propio con un clic; no aplica a la app local (L) | ❌ | C |
| 32 | **Pruebas puras de guardias, pipelines, memoria, secretos y prompt** (`*.test.ts`) | Llevar sus casos a pruebas Rust del host (S) | ❌ | B |

---

## 4. OpenDots (14 ideas)

| # | Idea (dónde está) | Cómo integrarla en Codync | Hoy | Código |
|---|---|---|---|---|
| 4 | **Esquema de la página** (`PageOutline.tsx`) | Panel lateral con los encabezados del documento en escritorio y Android (S) | ❌ | A (escritorio) |
| 7 | **Regla de instantáneas:** que una consulta antigua nunca borre un documento; las bajas son explícitas (`client/page-snapshots.ts`) | Aplicar a cualquier cliente que sondee: las mini-apps de Android sondean cada 3 s (S) | 🟡 | B |
| 8 | **Ejecutor con arrendamiento y «Interrumpida»:** si el servidor se detiene en una ejecución, queda marcada con «revisa los efectos antes de reintentar» (`server/runner.ts`) | Al arrancar el host, marcar pasos o rutinas `running` huérfanos como interrumpidos y pedir reintento explícito (S-M) | 🟡 Hay reintentos automáticos de rutinas | B |
| 10 | **Marcar mensajes de tareas programadas** con un prefijo estable que sobrevive a la reproducción del historial (`shared/scheduled-message.ts`) | Insignia «rutina» en los mensajes que dispara una rutina o un check-in (S) | 🟡 Hay avisos de ejecución de rutina en el chat; no una insignia en cada mensaje | B |
| 12 | **Rangos de IP no públicos incluyendo IPv6** (`browser/security.ts`: 100.64/10, 198.18/15, 3fff::/20, 2001:db8…) | Comparar con la lista de `egress.rs` y añadir lo que falte con pruebas (S) | 🟡 | B |
| 14 | **Llamadas en tiempo real con un agente de cómputo en paralelo** (`server/voice.ts`, `client/useVoice.ts`) | Mientras dura la llamada, lanzar el trabajo largo como turno normal del bot y narrar el resultado (M-L) | ❌ | C |
| 19 | **Aprendizaje automático:** contenedor de aprendizaje por conversación, solo el dueño, habilidades que se publican tras revisión (`server/learning.ts`, `shared/learning.ts`) | Pipeline: extraer candidatos de conversaciones → proponer en una bandeja → el usuario aprueba → `skills.rs` (L) | ❌ | C: depende de CopilotKit Intelligence |
| 21 | **Cómputo por bot en contenedores con perfil persistente** (`computer-service.ts`, `computer-store.ts`) | Solo si se quisiera una opción de computadora remota por bot (L) | ❌ | C |
| 22 | **Endurecimiento del supervisor Docker** (`deployment/computers/harden-supervisor.mjs`) | Lista de medidas útil si algún día hay sandbox Docker (M) | ❌ | C |
| 24 | **Tema oscuro derivado en la compilación del claro** (`build/dark-theme.ts`) | Generar los tokens oscuros de escritorio a partir de los claros (S) | ❌ | A |
| 25 | **Panel de resultados lateral** (`ResultPane.tsx`) | Los resultados de rutinas ya salen en el chat; un panel aparte sería nuevo en escritorio (M) | ❌ | C |
| 26 | **Embudo de activación:** etapas de preparación y «primera respuesta correcta» con un ID aleatorio (`setup-telemetry.ts`) | Añadir ese embudo a los eventos de PostHog (S) | 🟡 | B |
| 29 | **Documentar qué está verificado en vivo y qué solo con fixtures** (README) | Añadir esa columna a `docs/features/*.md` (S) | 🟡 | — |
| 30 | **Dos contenedores separados (app y navegador) con Docker Compose** | Solo para quien aloje Codync en un servidor (L) | ❌ | C |

---

## 5. Rakazo (27 ideas)

| # | Idea (dónde está) | Cómo integrarla en Codync | Hoy | Código |
|---|---|---|---|---|
| 1 | **Trabajo atascado:** aviso a las 4 h, cancela a las 24 h; una toma de control sin atender caduca a los **30 min** porque retiene la computadora del bot (`core/stuck-work.ts`) | Planificador en el host sobre estados «en cola», «esperando respuesta» y «esperando toma de control» (S-M) | ❌ | **A** para constantes y reglas (TypeScript puro, con pruebas); B para el host |
| 2 | **Importar un agente de otra herramienta:** lee ficheros de Hermes y OpenClaw, detecta credenciales y no las importa, limita archivos y tamaño (`core/agent-import/`, 14 archivos) | «Importar bot» en el escritorio: el proceso principal lee la carpeta y crea el bot con instrucciones, memoria y habilidades (M-L) | ❌ | **A**: los lectores en TypeScript corren en el proceso principal de Electron |
| 4 | **Pregunta tipo secreto** (`isSecretAskBlock`) | Misma herramienta de secretos de Coworkers (#5) | ❌ | B |
| 6 | **Guardia de secretos** (`secrets-guard.ts`) | Revisar qué detecta y si Codync lo hace en logs y respuestas (S) | ❌ | B |
| 7 | **Arrendamiento y reconexión de pantalla** (`screen-lease.ts`, `screen-reconnect.ts`) | Comparar con la propiedad de entrada de la pantalla remota de Codync (S) | 🟡 | B |
| 9 | **Avatares con vida:** movimiento, clips, rasgos y reacción a la actividad (`avatar-life.ts`, `avatar-motion.ts`, `logo-avatar-*`) | Animar el avatar del bot según el estado (trabaja, espera, termina) en los clientes (M) | 🟡 Hay personajes | B |
| 10 | **Texto humano de la actividad de herramientas** («Leyendo archivos…») (`tool-activity.ts`) | Resumen de una línea por herramienta en el chat y en la isla (S) | ❌ | A (escritorio); B (Kotlin y Swift) |
| 12 | **Limpiar Markdown antes de hablar** (`speech-text.ts`, `speech-locale.ts`) | Aplicar antes del TTS del dispositivo (S) | ❌ | **A** (TypeScript puro) |
| 13 | **Voces propias:** ElevenLabs, OpenAI, Cartesia, Fish Audio con tu clave (`adapters/voice-*`) | Añadir proveedores TTS en `host/src/voice.rs` con claves en la bóveda (M) | ❌ | B |
| 14 | **Mensajería:** iMessage/SMS/RCS (Sendblue), Slack, WhatsApp, Telegram con **varios bots con nombre**, Feishu (`messaging-providers.ts`, `adapters/lark`) | Abstraer un «canal» en el host sobre `slack/ingress.rs`; Telegram primero (L) | 🟡 Solo Slack | C |
| 15 | **Comandos de chat por mensajería** y prompts propios (`messaging-commands.ts`, `messaging-prompts.ts`) | `/stop`, `/estado`, `/rutinas` para los canales nuevos (S-M) | ❌ | B |
| 16 | **Agrupar menciones que llegan en ráfaga** antes de disparar una rutina entrante (`routine-mention-batcher.ts`, `inbound-routine.ts`) | Ventana de agrupación en los eventos de rutinas para no disparar 20 veces (S-M) | ❌ | B |
| 18 | **Punto de control de turno** (`turn-checkpoint.ts`) | Reanudar un turno tras caída del host sin repetir efectos; depende de lo que permita el agente ACP (L) | ❌ | C |
| 19 | **Diferencias entre versiones de instrucciones** (`instruction-diff.ts`) | Ver el diff al editar instrucciones; ya hay historial en `chat/instruction_history.rs` (S) | 🟡 | A (escritorio) |
| 21 | **Separador «mensajes nuevos»** (`unread-divider.ts`) | Línea «Nuevos mensajes» al volver a un chat (S) | ❌ | **A** (lógica pura con pruebas) |
| 23 | **Dirección de texto por mensaje** (`text-direction.ts`) | RTL por mensaje en lugar de por pantalla (S) | 🟡 | A y B |
| 25 | **Admisión de imágenes remotas** con límites (`thread-image-admission.ts`, `remote-images.ts`) | Pasar por `egress.rs` y limitar tamaño y tipo antes de mostrar (S) | 🟡 Los documentos no cargan recursos remotos; falta el límite para imágenes del chat | B |
| 26 | **Gráficas declarativas** (`plot/plot-spec.ts`): el bot emite una especificación y el cliente dibuja | Mini-app o bloque de chat que renderiza gráficas; el esquema valida el agente (M) | ❌ | **A** para el esquema en TypeScript; B para móvil |
| 28 | **Dispositivos del usuario como herramienta del bot** (`adapters/user-device-*`, `apps/api/user-devices.ts`) | El bot usa tu teléfono (pantalla, archivos) por el canal cifrado. Idea grande (L) | ❌ | C |
| 29 | **Roles de modelo:** qué modelo para chat, fondo y resumen (`model-role-settings.ts`) | Mismo concepto que el modelo de fondo de Coworkers (#10) pero con más roles (M) | ❌ | B |
| 30 | **Probar la conexión de un modelo antes de guardar** (`model-probe.ts`) | Botón «Probar» al añadir un agente o cuenta (S) | 🟡 `marketplace.md` menciona sondeos | B |
| 31 | **Consentimiento de IA en el primer uso** (`ai-consent.ts`) | Pantalla de consentimiento previa a enviar datos a un proveedor (S); útil para tiendas | ❌ | B |
| 32 | **Insignia con el número de no leídos en el icono** (`apps/desktop/src/badge.ts`) | Número en el icono de la bandeja o del Dock (S) | ❌ Solo hay el ajuste de mostrar en el Dock (`dock.ts`), no insignia de no leídos | **A** (Electron) |
| 33 | **Pruebas de extremo a extremo del escritorio** (`apps/desktop/e2e`: setup, smoke, arrastre de ventana) | Playwright para la app de escritorio (M) | ❌ Sin Playwright; hay pruebas e2e del host con agente falso | A |
| 34 | **Paquete de pruebas compartido con falsos de proveedores** (`packages/testkit`, 148 archivos) | Crate de pruebas con agentes y relays falsos para el host (M) | ❌ | C |
| 35 | **Instalación de un comando y `SETUP_PROMPT.md`** | `SETUP_PROMPT.md` para instalar Codync con un agente (S) | ❌ | **A** (texto) |
| 36 | **Revisión de PR por IA configurada** (`greptile.json`, CodeRabbit) | Opcional en el repo | ❌ | D |

---

## 6. Coucou (31 ideas)

| # | Idea (dónde está) | Cómo integrarla en Codync | Hoy | Código |
|---|---|---|---|---|
| 1 | **Máquina de estados pura de la isla:** oculta, pequeña, inicio, saludo, escuchando, sin AppKit (`IslandStateMachine.swift`) | Extraer el estado de `apps/desktop/src/main/island.ts` a una máquina pura con pruebas (S-M) | 🟡 | B |
| 2 | **Cola FIFO de aprobaciones:** la cabeza es la tarjeta visible; guarda sesión, herramienta, comando y una clave estable de entrada (`ApprovalQueue.swift`, 87 l) | Cola global de aprobaciones de todos los bots, con la cabeza en la isla y en el widget (M) | ❌ | B |
| 4 | **Aprobar desde la notificación** con acciones Permitir y Denegar (`DenyApprovalIntent.swift`, `NotificationViewController.swift`) | Acciones en la notificación de «te necesita» en iOS y Android; Permitir con biometría (M) | 🟡 Android sí (acciones en la notificación); iOS sin comprobar | B |
| 5 | **Preguntas con varias opciones:** hasta 4 preguntas, selección única o múltiple, y el Mac solo acepta etiquetas dentro de las opciones vigentes (`QuestionPayload.swift`, `QuestionCard.swift`) | Tipo de entrada «pregunta» en el host con esa validación; tarjeta en los cuatro clientes (M) | ❌ | B (copiar el contrato) |
| 7 | **Nueve widgets de Android:** lista, Mochi grande, ánimo, individual, píldora de estado, equipo, hoy, anillo de uso y alerta de espera (`android/app/.../*Widget.kt`, 779 líneas) | Entrada principal para el prompt de widgets v2. Usan `RemoteViews`, no Glance | 🟡 Hay 5 | **B**. Reaprovechar los 8 drawables por estado (`widget_card_attention/done/idle/muted/trouble/usage_calm`) y la lógica de `WidgetData.kt` |
| 8 | **Texto hablado de cada widget** para accesibilidad (`WidgetSpoken.kt`) | Descripción de contenido por tamaño y estado (S) | 🟡 iOS tiene etiquetas de accesibilidad; Android no | **A** (Kotlin, MIT) |
| 9 | **Resumen semanal** del lunes: turnos, archivos, líneas, comandos, preguntas y decisiones (`RecapStore.swift`, 330 l) | Calcularlo en el host desde SQLite y mostrarlo en los clientes (M) | ❌ | B |
| 10 | **Modo demo de 60 s sin red ni disco** (`DemoEngine.swift`, 756 l) | Sesión guiada por cliente, con datos de ejemplo en memoria (M) | ❌ | B |
| 11 | **Limpieza del dictado con un modelo pequeño y plazo de 4 s;** si falla, se pega el texto crudo (`FlowCleanup.swift`) | Opción de limpieza en el dictado de escritorio, con el mismo plazo y reserva (S-M) | ❌ | B |
| 12 | **Diccionario, reemplazos y snippets del dictado** (`FlowHubWords.swift`) | Lista del usuario aplicada al texto dictado (S-M) | ❌ | B |
| 13 | **Estadísticas locales del dictado** (`FlowHubInsights.swift`) | Palabras, tiempo ahorrado y racha, solo en el equipo (S) | ❌ | B |
| 14 | **Notas de llamadas sin bot** (`NoteTaker.swift`, `NoteAudio.swift`) | Capturar micrófono y audio del sistema, transcribir y resumir (L) | ❌ | C |
| 16 | **Signos vitales de la máquina** (`MachineVitals.swift`) | Mostrar RAM y CPU del equipo y avisar antes de lanzar compilaciones pesadas; útil con 8 GB (S-M) | ❌ | B |
| 17 | **Estado de ánimo reactivo** (`MochiMood.swift`) | El avatar cambia con la carga y los eventos (S) | ❌ | B |
| 18 | **Trajes y guardarropa** (`MochiWardrobe.swift`, `RenderOutfits.swift`) | Solo la idea de trajes por temporada; el arte es de su autor | ❌ | **D** (arte reservado) |
| 19 | **Mochi suelto en el escritorio** (`DesktopMochi.swift`) | Avatar flotante en Electron (M) | ❌ | B |
| 20 | **Soltar un archivo en la isla** (`FileDropView.swift`) | Zona de arrastre en la isla para adjuntar al chat (S-M) | ❌ | B |
| 21 | **Captura de pantalla para el agente** (`ScreenCapture.swift`, `CaptureCardView.swift`) | Botón «adjuntar ventana o pantalla» con `desktopCapturer` (M) | ❌ | B |
| 22 | **Palabra de activación y voz «Jarvis»** (`ClaudeVoiceBrain.swift`, `IntentParser.swift`) | Detección de palabra clave en el dispositivo (L) | ❌ | C |
| 23 | **Píldoras de servicios** con sondeo y detalle: GitHub, n8n, Notion, Resend, Cal.com (`GithubPoller.swift`, `ServiceDetailRunner.swift`, `ServicePublisher.swift`) | Patrón en el host: sondeo con claves de la bóveda, instantánea y detalle con acciones seguras (L) | ❌ | B |
| 24 | **Abrir solo enlaces http/https con host** (`SafeWebURL.swift`, 12 l) | Aplicar la misma regla en todos los clientes al abrir enlaces que vengan del modelo (S) | 🟡 `app:openExternal` valida los esquemas, pero el manejador de `window.open` del escritorio abre cualquier URL (`main/index.ts:91`) | B (reglas) |
| 25 | **Spotlight** (`SearchAndSpotlight.swift`) | Indexar turnos pasados en Spotlight en iOS (M) | ❌ | B |
| 26 | **Atajos de Siri** (`CoucouIntents.swift`) | Ya hay `OpenBotIntent.swift`; ampliar con «¿cómo van mis bots?» (S-M) | 🟡 | B |
| 27 | **Control de Control Center** (`CoucouControl.swift`) | Control de iOS 18 para abrir el bot líder (S-M) | ❌ | B |
| 28 | **Vectores de prueba compartidos entre cliente y relay** (`android/relay/test-vectors.json`, `tools/gen-vectors.mjs`) | Vectores del cifrado E2E del canal para host, Android e iOS (M) | ❌ | **A** el método; B los datos |
| 29 | **Aprobar con huella y mostrar sobre otras apps** (`android/app`) | La huella ya está; la superposición sobre otras apps sería nueva (M) | 🟡 | B |
| 30 | **Icono de ajustes rápidos y notificación continua en Android** | Superficie de estado rápida de los bots (S-M) | 🟡 Hay icono de ajustes rápidos (`LeadBotTile.kt`); faltan la isla y la notificación fija | B |
| 31 | **Traducciones a 14 idiomas con árabe de derecha a izquierda** (`android/i18n/translations`, `REVIEW.md`) | Base de partida para los idiomas que falten en Codync, revisando la calidad (M) | 🟡 Hay 7 idiomas (de, es, fr, ja, ko, zh-Hans, zh-Hant) frente a 14 | **A** (datos, MIT) |
| 32 | **Atajos globales configurables** (`HotKeyCenter.swift`, `ShortcutLogic.swift`) | Pantalla de atajos en escritorio (S-M) | 🟡 | B |
| 33 | **Sonidos por evento reemplazables por el usuario** (`SoundEngine.swift`) | Carpeta de sonidos del usuario; sin usar los audios de Coucou (S) | 🟡 | B. Sonidos: **D** |
| 34 | **Indicador de versión nueva** (la mascota cambia de color) | Cambiar el color del avatar de la isla cuando hay actualización (S) | 🟡 Hay botón de actualizar | B |

---

## 7. Mercury Agent (34 ideas)

| # | Idea (dónde está) | Cómo integrarla en Codync | Hoy | Código |
|---|---|---|---|---|
| 1 | **Veredicto de fin de turno:** un turno que acaba por agotar los pasos es una **pausa**, no un éxito (`core/completion-verdict.ts`) | Distinguir «terminó» de «se quedó sin pasos o se cortó» en el estado del bot y en las notificaciones (M) | ❌ | B, con sus casos |
| 2 | **Reanudación automática acotada** tras agotar pasos | Un reintento limitado con aviso (S-M) | ❌ | B |
| 3 | **«Hecho» solo con evidencia:** una compilación, prueba o typecheck con código de salida 0 (`execute-guard.ts`, `programming-mode.ts`). Su ROADMAP admite que fallaba por comparar texto en vez de salida | Política por bot: marcar éxito solo si el último comando de verificación terminó en 0 (M) | ❌ | B (mejor diseñado que su versión) |
| 5 | **Registro de trabajo con escritura atómica y recuperación tras caída** (`core/work-ledger.ts`, 347 l) | Reencolar mensajes aceptados pero no entregados tras reiniciar el host (M) | ❌ | B |
| 6 | **Marca de caída** (`crash-flag.ts`) | Al arrancar tras una caída, avisar en el chat «me reinicié» (S) | 🟡 Hay reportes de fallos; falta el aviso «me reinicié» en el chat | B |
| 7 | **Gobernador de memoria por paso** (`memory-governor.ts`, `memory-guard.ts`): respuesta graduada según el crecimiento | Vigilar el consumo de los procesos de agente y avisar o cerrar los que se disparan; muy útil con 8 GB (M) | ❌ Sin medición de memoria de los procesos de agente | B |
| 8 | **Modo ahorro:** recorta contexto y manda tareas rutinarias a un modelo barato (`saver-mode.ts`) | Interruptor por bot sobre `budget.rs` (M) | ❌ | B |
| 9 | **Herramienta para consultar el presupuesto** (`system/budget-status.ts`) | Que el bot lea su gasto del día (S) | ❌ | B |
| 10 | **Límites de ejecución por turno** (`execution-limits.ts`) | Pasos y tiempo máximos configurables por bot (S) | 🟡 Hay tiempos máximos en rutinas y turnos; sin límite de pasos configurable | B |
| 11 | **Niveles de permiso al crear un bot:** solo lectura, constructor, operador y total, con alcance de carpetas que se aplica de verdad (`bots/permission-tiers.ts`) | Una sola pregunta al crear el bot que escribe `permission` y `approvalRules` coherentes (S-M) | ❌ Hoy es «preguntar» o «auto» | B |
| 12 | **Alcance de rutas por carpeta** (`filesystem/approve-scope.ts`) | No aplica a agentes externos por ACP; sí a archivos que maneja Codync | ❌ | C |
| 13 | **Lista de comandos prohibidos que no se puede anular**, con variantes de PowerShell para borrar la raíz o el home (`capabilities/shell/blocklist.ts`) | Lista fija en el host que niega la aprobación automática aunque el bot esté en «auto» (S) | ❌ | **A** para las listas (son datos); B para el motor |
| 14 | **Carril de argumentos** (`shell/argv-lane.ts`) y aprobar comando (`approve-command.ts`) | Analizar el comando por segmentos como ya hace `approval_rules.rs` con `;`, `&&`, ` | ` (S) | 🟡 | B |
| 15 | **Segundo cerebro:** 10 tipos, confianza e importancia, consolidación cada hora, caducidad | Etiquetas de tipo y confianza en las notas de memoria; dreaming ya consolida (L) | 🟡 | C |
| 16 | **Confirmar, corregir y olvidar creencias** (`system/confirm-belief.ts`, `correct-belief.ts`, `forget-memory.ts`) | Herramientas y tarjeta «¿sigue siendo cierto…?» (M) | ❌ | B |
| 17 | **Anunciar lo aprendido:** una línea como máximo por turno; si hay contradicción, una pregunta como máximo al día (`learning/announce.ts`) | Aviso breve «aprendí X» con botón de deshacer, y límite de una pregunta diaria (S-M) | ❌ | B |
| 18 | **Alma, personalidad, gusto y latido en Markdown** (`soul/`, `soul.md`, `persona.md`, `taste.md`, `heartbeat.md`) | Capas de instrucciones por bot editables, con historial como la memoria (S-M) | ❌ | B |
| 19 | **Diario del bot** (`bots/journal.ts`) | Registro cronológico de lo que hizo cada bot, legible por el usuario (M) | ❌ | B |
| 20 | **Retención y limpieza** (`bots/retention.ts`) | Política de borrado de historial y resultados antiguos (S-M) | ❌ | B |
| 21 | **Sintetizar una habilidad** cuando una ejecución usó 3 o más herramientas distintas (`bots/skill-synthesis.ts`) | Proponer «guardar como habilidad» al terminar una rutina compleja (M) | ❌ | B |
| 22 | **Tareas tipadas de flota y entrega de archivos** (`bots/tools/fleet-tasks.ts`, `bot-deliver.ts`) | Carpeta de entregables por bot y envío al canal (M) | 🟡 | B |
| 25 | **Subagentes** (`capabilities/subagents/*`) | Los agentes ACP ya delegan; falta una vista de «sub-trabajos» | 🟡 Hay delegación en equipos y eventos de subagentes; sin vista propia | B |
| 26 | **Tablero de tareas tipo Kanban** (`core/task-board.ts`, `board-manager.ts`, `web/`) | Vista Kanban de rutinas y trabajo de los bots (L) | ❌ | C |
| 27 | **Filtro de alertas** (`core/alert-gate.ts`) | Agrupar y limitar notificaciones por importancia (S-M) | 🟡 | B |
| 28 | **Pulso de progreso y verbos de estado** (`progress-pulse.ts`, `status-verbs.ts`) | Texto de estado variado en el chat mientras trabaja (S) | 🟡 | A (escritorio); B (otros) |
| 29 | **Contrato de retroalimentación común:** iniciado, paso, esperando, hecho, fallado, igual en todos los canales (ROADMAP) | Escribirlo como regla de diseño para los canales nuevos (S) | ❌ | — |
| 30 | **Slack por Socket Mode:** el cliente abre un WebSocket saliente, sin URL pública (`channels/`) | Quitar el requisito de un endpoint HTTPS público del canal de Slack de Codync, que hoy lo exige (`docs/features/slack-channel.md`) (M) | ❌ | B (el host ya trae WebSocket) |
| 31 | **Telegram con botones, entrega de archivos y emparejamiento por código** (`channels/telegram`) | Canal nuevo sobre `slack/ingress.rs` (L) | ❌ | B |
| 32 | **Discord y Signal** (`channels/`, `signal/`) | Discord con comandos «/» y roles; Signal con `signal-cli` (L) | ❌ | B / C |
| 33 | **Acompañante de WhatsApp en Rust** (`plugins/whatsapp/companion`, crate `mercury-wa`, MIT, Rust 2024, sobre `whatsapp-rust`) | Evaluar como pasarela externa del host. Es el mismo lenguaje; el propio autor avisa que el protocolo no es oficial (L) | ❌ | **A** para evaluarlo; riesgo de baneo por protocolo no oficial |
| 33b | **Acompañante de voz en Rust** (`plugins/optimus/companion`: audio, atajo, STT, Gemini en tiempo real, tubería) | Candidato para dictado y voz de escritorio fuera de línea o de proveedor (L) | 🟡 | **A** para evaluarlo |
| 34 | **Escenarios de evaluación del agente** (`src/eval/scenarios`) | Casos de evaluación para los prompts de Codync, como la evaluación de memoria M4 (M) | 🟡 | B |
| 35 | **Registro de decisiones y auditoría con `archivo:línea`** (`DECISIONS.md`, `ROADMAP.md`) | Añadir `docs/DECISIONS.md` con las decisiones y su motivo (S) | ❌ | — |
| 36 | **Comandos rápidos sin modelo** (`commands/fast-path-command.ts`) | «/estado», «/presupuesto» resueltos por el host sin gastar tokens (S) | ❌ | B |

---

## 8. Las mejores ideas de todas, en orden

Ordenadas por valor frente a esfuerzo, usando lo que Codync todavía no tiene.

| Orden | Idea | Fuente | Esfuerzo | Código |
|---|---|---|---|---|
| 1 | Aprobación sin respuesta: aviso a las 4 h, cierre a las 24 h y toma de control a los 30 min | Rakazo #1, Coworkers #2 | S-M | A (constantes) + B |
| 2 | Lista de comandos prohibidos que ni «auto» puede saltarse | Mercury #13 | S | A (datos) |
| 3 | Modelo de fondo más barato y modo ahorro | Coworkers #10, Mercury #8, Rakazo #29 | M | B |
| 4 | Niveles de permiso al crear un bot | Mercury #11 | S-M | B |
| 5 | Veredicto de fin de turno («pausa» frente a «éxito») y «hecho» con evidencia | Mercury #1, #3 | M | B |
| 6 | Cola global de aprobaciones en isla y widget | Coucou #2 | M | B |
| 7 | Preguntas con opciones múltiples, validadas | Coucou #5, Coworkers #20 | M | B |
| 8 | Slack por Socket Mode, sin endpoint público | Mercury #30 | M | B |
| 9 | Secretos atados a un origen HTTPS | Coworkers #5, #6 | M | B |
| 10 | Widgets de Android: 9 widgets con colores por estado | Coucou #7, #8 | M-L | B (drawables A) |
| 11 | Separador «Nuevos mensajes» | Rakazo #21 | S | A |
| 12 | Importar bots de otras herramientas | Rakazo #2 | M-L | A (en el proceso principal) |
| 13 | Confirmar, corregir y anunciar lo aprendido en la memoria | Mercury #16, #17 | M | B |
| 14 | Resumen semanal y modo demo | Coucou #9, #10 | M | B |
| 15 | Pipelines con pasos de aprobación humana (rutinas por nodos) | Coworkers #3 | L | B |
| 16 | Gobernador de memoria de procesos (útil con 8 GB) | Mercury #7, Coucou #16 | M | B |
| 17 | Personalidad en archivos Markdown | Mercury #18 | S-M | B |
| 18 | Gráficas declarativas en el chat y las mini-apps | Rakazo #26 | M | A (esquema) |
| 19 | Telegram como segundo canal | Mercury #31, Rakazo #14 | L | B |
| 20 | Proponer habilidades al terminar rutinas complejas | Mercury #21, OpenDots #19 | M | B |

## 9. Lo que no conviene

- **CloudKit como reemplazo del relay:** solo funciona en dispositivos Apple; como modo opcional sí (ver `ideas-nuevas-y-grandes-2026-10-28.md`, sección 5).
- **Arte y sonidos de Coucou** (Mochi, trajes, 28 sonidos): reservados por su autor.
- **El núcleo en Cloudflare o Postgres** (`bot-agent.ts`, `user-hub.ts`, `packages/db`, el motor de aprendizaje de OpenDots): no encaja con una app local de una computadora.
- **WhatsApp por protocolo no oficial** (Mercury #33): útil pero con riesgo de bloqueo de cuenta; solo como opción experimental.

## 10. Lo que no pude comprobar

- No ejecuté ni compilé el código de ninguno de los cinco repos; leí archivos y estructura.
- Los conteos de líneas son de los archivos clonados el 2026-10-28.
- Un «❌» significa que no lo encontré con `git grep`. Quedan por revisar a fondo `chat/teach.rs`, `chat/skills.rs`, la lista de IP de `egress.rs`, el comportamiento actual de las notificaciones de aprobación en iOS y qué cubre el dictado de escritorio.
- Los widgets de Coucou los vi por sus archivos y tamaños, no los instalé.
