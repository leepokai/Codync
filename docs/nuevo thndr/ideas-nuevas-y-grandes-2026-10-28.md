# Ideas nuevas y grandes para Codync (2026-10-28)

Sustituye al documento anterior `ideas-30-por-repo-como-integrarlas-2026-10-28.md`, que quedó en el historial de git. Aquí solo hay ideas que **Codync no tiene**
y que son **grandes**: pantallas o experiencias nuevas, capacidades de producto y cambios de fondo. Las mejoras
pequeñas y lo que ya existe quedaron fuera (sección 7).

Fuentes: el código de Coworkers/bdots, OpenDots, Rakazo, Coucou y Mercury Agent, clonados en
`C:\Users\Edwin Villeda\Documents\ref-repos\` el 2026-10-28. Los cinco tienen archivo `LICENSE` (MIT; Apache-2.0 el
de Rakazo), así que su código se puede reutilizar con sus avisos; el nombre y el arte de Coucou están reservados.

## 0. Cómo leer esto

- **Tamaño:** M = una a dos semanas · L = más de dos semanas · XL = es un proyecto en sí mismo.
- **Código de referencia:** **A** copiar y adaptar (mismo lenguaje y licencia que permite) · **B** portar el diseño y
  las pruebas, reescribiendo (casi todo el host de Codync es Rust) · **C** solo la idea (depende de su infraestructura).
- **Hoy:** lo que encontré en Codync con `git grep` en el host, escritorio, Android, iOS y `docs/features`. Si una idea
  tiene algo parecido, lo digo.
- Los **hooks** y **CloudKit** van en las secciones 4 y 5 aunque `CLAUDE.md` diga lo contrario: es un proyecto
  personal y quedan como ideas. Dato útil: Codync **ya tiene hooks** para sesiones externas
  (`host/src/external/`, `codync-host hooks install`); `CLAUDE.md` quedó desfasado en eso. CloudKit sí es nuevo.

---

## 0.1 ¿Vale la pena usar el código de cada repo?

| Repo | Pila y licencia | Veredicto |
|---|---|---|
| **Coworkers (bdots)** | TypeScript sobre Cloudflare Workers · MIT | **Parcial.** Se puede copiar lo puro: los esquemas y la validación de las tuberías, el script de grabación de demostraciones y las pruebas. No usar `bot-agent.ts` (5 107 líneas, un Durable Object) ni `user-hub.ts` |
| **OpenDots** | TypeScript, depende de CopilotKit Intelligence · MIT | **Poco.** Casi todo lo grande depende de ese servicio; sirven las ideas |
| **Rakazo** | Monorepo TypeScript, Postgres, Expo · Apache-2.0 | **Sí, en piezas puras:** `agent-import/`, `plot-spec.ts`, `stuck-work.ts`. Apache-2.0 exige conservar el `NOTICE` y marcar los archivos modificados |
| **Coucou** | Swift (Mac/iOS), Kotlin (Android), Rust/Tauri (Windows) · MIT; arte reservado | **Parcial.** Los hooks de Windows están en Rust y se pueden aprovechar; sus widgets de Android usan `RemoteViews` clásicos (no Glance como Codync), así que se porta la idea. Nunca su arte ni sonidos |
| **Mercury** | TypeScript (un agente de 8 362 líneas) · MIT | **Sí en tres cosas:** la lista de comandos prohibidos, los módulos puros como especificación con pruebas y los acompañantes en **Rust** (mismo lenguaje que tu host) |

**Reglas de uso:** conservar el aviso de copyright de cada archivo copiado, añadir el origen a un archivo de atribuciones de terceros y dejar una línea
de origen en la cabecera del archivo, como ya hacen `budget.rs` y `egress.rs`.

---

## 1. Pantallas y experiencias nuevas

### 1.1 Tablero Kanban del trabajo de tus bots
- **Fuente:** Mercury (`core/task-board.ts`, `board-manager.ts`, `board-db.ts`, `task-surface.ts`; interfaz en `ui/`, con su README
  mencionando «Chat, Kanban, bots cockpit»).
- **Qué verías:** columnas (por hacer, en curso, esperando tu respuesta, hecho) con las tareas de todos los bots, las rutinas
  que corren y las decisiones pendientes; arrastras una tarjeta para reasignarla o pausarla.
- **Cómo en Codync:** vista nueva sobre datos que el host ya tiene (rutinas, elementos abiertos, aprobaciones, tareas de
  equipos); el estado de cada tarjeta sale del hub por los eventos con `rev`. Aparece en escritorio, Android, iOS y, como lista,
  en la terminal.
- **Hoy:** no existe (búsqueda de «kanban» sin resultados). **Tamaño:** L. **Código:** C (las ideas), B (el modelo de datos).

### 1.2 Cabina web: usar tus bots desde un navegador
- **Fuente:** Mercury (`src/web/api`, 25 archivos; `ui/` con 38 componentes y 21 páginas), Rakazo (`apps/web`, 597 archivos), Coworkers (app web).
- **Qué verías:** abres `https://…` en cualquier navegador, inicias sesión y tienes tu lista de bots, chat, aprobaciones y rutinas,
  sin instalar nada. Útil en una computadora prestada o en un Chromebook.
- **Cómo en Codync:** cliente web que habla con el host por el canal cifrado extremo a extremo (WebCrypto en el navegador) a través del
  relay; la API de loopback sigue cerrada. Reutiliza los modelos del escritorio (TypeScript).
- **Hoy:** no hay cliente web; la carpeta `web/` es el sitio estático de Codync. **Tamaño:** XL. **Código:** A para componentes
  de chat en TypeScript; B para el cifrado.

### 1.3 Gráficas y visualizaciones declarativas
- **Fuente:** Rakazo (`packages/core/src/plot/plot-spec.ts`; no leí el archivo completo).
- **Qué verías:** el bot responde con una gráfica de barras, líneas o dispersión dentro del chat y de las mini-apps, interactiva, en lugar de
  una tabla de texto.
- **Cómo en Codync:** el bot emite una especificación JSON validada por un esquema; los clientes la dibujan (escritorio con una
  librería de gráficas, Android con Canvas, iOS con Swift Charts). Las mini-apps la recibirían como componente.
- **Hoy:** no hay librería de gráficas en el escritorio. **Tamaño:** M-L. **Código:** A para el esquema; B para móvil.

### 1.4 Panel de salud de tu computadora y gobernador de procesos
- **Fuente:** Coucou (`MachineVitals.swift`), Mercury (`core/memory-governor.ts`, `memory-guard.ts`: respuesta graduada según el crecimiento).
- **Qué verías:** RAM, CPU y disco en la isla y en la pestaña de estado, y un aviso «esto va a ahogar tu PC» antes de lanzar tres bots a la
  vez. El host baja la prioridad o cierra el agente que se dispara. Muy útil en una máquina de 8 GB.
- **Cómo en Codync:** el host (Rust, con `sysinfo`) mide el consumo de cada proceso de agente y aplica una política graduada; los clientes
  solo muestran.
- **Hoy:** no existe. **Tamaño:** M-L. **Código:** B.

### 1.5 Estudio de dictado («Flow»)
- **Fuente:** Coucou (`FlowDictation.swift`, `FlowHub*.swift`, `FlowPill.swift`, `FlowCleanup.swift`, `EnergyVAD.swift`, `ParakeetEngine.swift`).
- **Qué verías:** mantienes una tecla en cualquier app, hablas y se pega el texto donde escribes. Una píldora semitransparente muestra el audio.
  Diccionario de nombres propios, reemplazos y snippets («di una frase corta y pega un texto largo»), limpieza opcional con un modelo
  pequeño (quita muletillas y autocorrecciones; si tarda más de 4 s pega el texto crudo) y estadísticas locales (palabras, tiempo ahorrado, racha).
- **Cómo en Codync:** ampliar el dictado de escritorio (`apps/desktop/src/renderer/views/thread/dictation.ts`) a nivel de sistema; en Windows el
  motor fuera de línea tendría que ser otro (el de Coucou usa el Neural Engine de Apple).
- **Hoy:** hay dictado dentro del chat; no hay uso global, diccionario ni limpieza. **Tamaño:** L. **Código:** B.

### 1.6 Adjuntar una ventana o la pantalla al chat
- **Fuente:** Coucou (`ScreenCapture.swift`, `CaptureCardView.swift`).
- **Qué verías:** botón «adjuntar ventana»: eliges una ventana o la pantalla, aparece una tarjeta de captura y el bot la ve.
- **Cómo en Codync:** `desktopCapturer` de Electron en el proceso principal, la imagen entra como adjunto del mensaje.
- **Hoy:** no hay captura de pantalla hacia el chat (sin resultados con `desktopCapturer`). **Tamaño:** M. **Código:** B.

### 1.7 «Tu semana con Codync» y novedades tras actualizar
- **Fuente:** Coucou (`RecapStore.swift`, 330 l: por turno, archivos cambiados, líneas añadidas y quitadas, comandos, preguntas y decisiones).
- **Qué verías:** cada lunes una tarjeta con el resumen de la semana (tiempo trabajado por los bots, sesiones, archivos y líneas, permisos que
  diste, el bot y el proyecto más activos, el día de más trabajo) y, tras cada actualización, «qué hay de nuevo» y «tu Codync hasta ahora».
- **Cómo en Codync:** el host calcula el resumen desde SQLite (sin modelo, sin red); los clientes lo muestran y lo pueden compartir.
- **Hoy:** no existe. **Tamaño:** M. **Código:** B.

### 1.8 Modo demo guiado
- **Fuente:** Coucou (`DemoEngine.swift`, 756 l: un recorrido de unos 60 s «sin red ni disco»).
- **Qué verías:** «Probar el modo demo» y una sesión simulada con pasos, un diff y un permiso, sin API, sin agente y sin escribir nada.
- **Cómo en Codync:** datos de ejemplo en memoria en cada cliente, con un interruptor visible; ideal para revisores de tienda y para
  quien instala Codync sin ningún agente.
- **Hoy:** no existe. **Tamaño:** M. **Código:** B.

### 1.9 Tableros de tus servicios (GitHub, Vercel, Stripe, Cal.com, n8n, Notion, Resend)
- **Fuente:** Coucou (`GithubPoller.swift`, `N8nPoller.swift`, `NotionPoller.swift`, `ResendPoller.swift`, `CalcomPoller.swift`,
  `ServicePublisher.swift`, `ServiceSnapshot.swift`, `ServiceDetailRunner.swift`).
- **Qué verías:** tarjetas con lo que pasa en tus cuentas (PRs abiertos, CI en curso, despliegues, ingresos del día) y acciones seguras
  como «reejecutar CI». Cada servicio con su color propio.
- **Cómo en Codync:** el host consulta las APIs con claves de la bóveda (patrón: sondeo → instantánea → detalle con acciones) y los
  clientes dibujan las tarjetas; los bots también las podrían leer como herramienta.
- **Hoy:** no encontré tableros de servicios para el usuario (Composio conecta apps a los bots, que es otra cosa). **Tamaño:** L. **Código:** B.

### 1.10 Mascota con vida, suelta en el escritorio
- **Fuente:** Rakazo (`avatar-life.ts`, `avatar-motion.ts`, `avatar-clips.ts`, `logo-avatar-*`), Coucou (`DesktopMochi.swift`, `MochiMood.swift`).
- **Qué verías:** el avatar del bot parpadea, respira y cambia de ánimo según el trabajo (concentrado, esperando, terminó, algo falló); puedes
  arrastrarlo fuera de la isla y deja un compañero de 120 px que sigue tu cursor y vuelve volando cuando el bot te necesita.
- **Cómo en Codync:** sistema de animaciones por estado sobre los personajes que ya existen (clips cortos por estado); ventana flotante
  transparente en Electron. Nada del arte de Coucou: sus trajes, nombre y sonidos están reservados.
- **Hoy:** hay personajes y avatares, sin esa vida ni versión suelta. **Tamaño:** M-L. **Código:** B.

### 1.11 Superficies de estado en Android (isla, ajustes rápidos, notificación continua)
- **Fuente:** Coucou Android (según su README: una isla que baja del recorte de la cámara mientras un agente trabaja, un icono de ajustes
  rápidos y una notificación fija con las acciones principales).
- **Qué verías:** de un vistazo y sin abrir la app, qué bot trabaja y cuál te necesita, con un botón para abrir la aprobación.
- **Cómo en Codync:** servicio en primer plano con un overlay opcional, un `TileService` y la notificación; datos del `WidgetFeed`.
- **Hoy:** hay widgets y estado en la app; no encontré isla ni icono de ajustes rápidos. **Tamaño:** M. **Código:** B.

### 1.12 iOS a fondo: Control Center, Spotlight, Focus y Mochi que viaja
- **Fuente:** Coucou iPhone (`CoucouControl.swift`, `SearchAndSpotlight.swift`, `CoucouIntents.swift`, `LiveActivityRelay.swift`).
- **Qué verías:** un control en el Centro de control para abrir tu bot líder; buscas en Spotlight lo que dijo un bot hace tres semanas; un
  filtro de Focus que muestra solo los bots que te esperan; y al bloquear el Mac mientras un agente trabaja, la actividad pasa a tu iPhone.
- **Cómo en Codync:** extensiones de iOS 18 sobre los intents que ya existen (`OpenBotIntent.swift`); indexación local de turnos.
- **Hoy:** hay intents y Live Activity; no encontré Control Center ni Spotlight. **Tamaño:** M-L. **Código:** B (solo con Mac).

### 1.13 Llamada de voz con un agente de cómputo en paralelo
- **Fuente:** OpenDots (según su README: «la llamada empareja habla en tiempo real con un agente de cómputo aparte, así la conversación sigue
  mientras corre el trabajo largo»; `server/voice.ts`, `client/useVoice.ts`).
- **Qué verías:** le dices «revisa esos tres repos» y sigues hablando; el bot avisa por voz cuando termina, con el resultado en la línea de tiempo.
- **Cómo en Codync:** la llamada lanza el trabajo largo como un turno normal del bot (ACP) y narra el avance; mismo contexto y permisos.
- **Hoy:** hay llamadas de voz con recibos; no vi trabajo en paralelo. **Tamaño:** L. **Código:** C.

### 1.14 Espacios compartidos entre varios bots
- **Fuente:** OpenDots (según su README: un bot puede tener acceso a varios Espacios y cada documento guarda en uno).
- **Qué verías:** una biblioteca común donde varios bots leen y escriben (investigación, borradores, notas), con permisos por espacio.
- **Cómo en Codync:** hoy los documentos son por bot o grupo (`documents.rs`); añadir un nivel «Espacio» con permisos de acceso por bot.
- **Hoy:** documentos por bot o grupo. **Tamaño:** L. **Código:** C.

---

## 2. Capacidades de producto y de agentes

### 2.1 Canales de mensajería: Telegram, Discord, Signal, WhatsApp e iMessage
- **Fuente:** Mercury (`src/channels/`, `src/signal/`: Telegram con botones, archivos y emparejamiento por código; Discord con comandos «/»;
  Signal con `signal-cli`), Rakazo (`messaging-providers.ts`: iMessage/SMS/RCS por Sendblue, WhatsApp, Telegram con **varios bots con nombre**,
  Feishu), Coucou (WhatsApp con pasarela).
- **Qué verías:** hablas con tus bots desde Telegram o donde ya estés, apruebas permisos con botones y recibes archivos y resultados de
  rutinas, sin abrir Codync.
- **Cómo en Codync:** abstraer un «canal» en el host a partir de `host/src/slack/` (entrada firmada, lista de permitidos, trabajador); Telegram primero;
  emparejamiento por código y las aprobaciones por botones.
- **Hoy:** solo Slack (y sin probar con un workspace real); nada de Telegram, Discord, Signal ni WhatsApp. **Tamaño:** L por canal. **Código:** B;
  y para WhatsApp, el acompañante en Rust de Mercury (ver 2.12).
- **Extra:** Slack por **Socket Mode** (conexión saliente por WebSocket) quitaría el endpoint HTTPS público que Codync exige hoy. Tamaño M.

### 2.2 Tuberías de bots con pasos de aprobación humana
- **Fuente:** Coworkers (`agents/pipelines.ts`: hasta 10 pasos, de bot o de aprobación; `components/pipeline-editor.tsx`, `pipeline-runs.tsx`).
- **Qué verías:** un editor de pasos («investiga → redacta → *tú apruebas* → publica») con el historial de cada ejecución.
- **Cómo en Codync:** es el modelo de las rutinas por nodos con un nodo «aprobación». **Ya tienes dos prompts preparados** para el
  motor y las pantallas de rutinas por nodos; este paso humano encajaría dentro.
- **Hoy:** no existe. **Tamaño:** L. **Código:** B.

### 2.3 Importar agentes de otras herramientas
- **Fuente:** Rakazo (`core/agent-import/`: lectores de Hermes y OpenClaw, detección de credenciales que no se importan, límites de archivos y tamaño).
- **Qué verías:** «Importar un agente»: eliges la carpeta de otra herramienta y Codync crea el bot con sus instrucciones, memoria y habilidades.
- **Cómo en Codync:** los lectores en TypeScript se pueden copiar al proceso principal de Electron, que crea el bot por la API del host.
- **Hoy:** no existe. **Tamaño:** M-L. **Código:** **A**.

### 2.4 Tus dispositivos como herramienta del bot
- **Fuente:** Rakazo (`adapters/user-device-gateway.ts`, `user-device-client.ts`, `user-device-protocol.ts`, `apps/api/user-devices.ts`; no leí los detalles).
- **Qué verías:** el bot usa tu teléfono (leer una foto, enviar una notificación, abrir una app) con aprobación, y tu PC si trabajas desde el teléfono.
- **Cómo en Codync:** un canal de «herramientas del dispositivo» sobre el canal cifrado existente, con permisos por herramienta.
- **Hoy:** existe el control remoto de pantalla; no existe «dispositivo como herramienta». **Tamaño:** XL. **Código:** C.

### 2.5 Computadoras en la nube por bot
- **Fuente:** Rakazo (proveedores E2B, Daytona, CreateOS, Docker y otros; computadoras privadas o compartidas del equipo), OpenDots (contenedores con
  perfil de navegador y archivos persistentes), Coworkers (Browser Run en Cloudflare).
- **Qué verías:** un bot con su propia computadora remota, que sigue trabajando aunque apagues tu PC.
- **Cómo en Codync:** un backend de «computadora» más junto al local y SSH; el panel de computadora y las herramientas del host ya existen.
- **Hoy:** no existe. **Tamaño:** XL. **Código:** C.

### 2.6 Bandeja de habilidades propuestas
- **Fuente:** OpenDots (`server/learning.ts`: aprendizaje por conversación y publicación tras revisión), Mercury (`bots/skill-synthesis.ts`: si
  una ejecución usó 3 o más herramientas distintas, propone guardarla como habilidad).
- **Qué verías:** al terminar una rutina compleja, una tarjeta «¿guardo esto como habilidad?» con el procedimiento redactado; una bandeja para
  aprobar, editar o descartar antes de que el bot lo use.
- **Cómo en Codync:** el host detecta ejecuciones con varias herramientas, genera un borrador con el agente y lo guarda con `skills.rs` tras tu aprobación.
- **Hoy:** hay habilidades y `teach.rs`; no hay propuestas automáticas. **Tamaño:** L. **Código:** B.

### 2.7 «Lo que sé de ti»: memoria como creencias que se confirman y corrigen
- **Fuente:** Mercury (`system/confirm-belief.ts`, `correct-belief.ts`, `forget-memory.ts`, `learning/announce.ts`: una línea por turno como máximo y una
  pregunta de contradicción como máximo al día).
- **Qué verías:** una pantalla con lo que cada bot cree de ti (preferencias, proyectos, decisiones) con confianza; «¿sigue siendo cierto?», un botón
  de corregir u olvidar, y avisos breves «aprendí que prefieres X» con deshacer.
- **Cómo en Codync:** capa de metadatos (tipo, confianza, última confirmación) sobre las notas de memoria en archivos y herramientas del bot para
  confirmar, corregir y olvidar.
- **Hoy:** memoria en archivos con versiones y «dreaming», sin tipos ni confirmación. **Tamaño:** L. **Código:** B.

### 2.8 Fin de turno honesto y «hecho» con evidencia
- **Fuente:** Mercury (`core/completion-verdict.ts`: un turno que acaba por agotar los pasos es una **pausa**, no un éxito; `execute-guard.ts`,
  `programming-mode.ts`: «hecho» solo con una compilación o prueba que terminó en código 0).
- **Qué verías:** en vez de «terminó» cuando el agente se quedó sin pasos, «pausado a la mitad, ¿continúo?»; y en bots de código, «hecho»
  solo si las pruebas pasaron.
- **Cómo en Codync:** distinguir en el estado del bot «terminó», «se quedó sin pasos» y «se cortó»; política opcional por bot que lee el resultado
  del último comando de verificación.
- **Hoy:** no existe. **Tamaño:** M-L. **Código:** B (su propio ROADMAP admite que comparaba texto y no salida: hazlo con el código de salida).

### 2.9 Perfiles de seguridad al crear un bot
- **Fuente:** Mercury (`bots/permission-tiers.ts`: solo lectura, constructor, operador y total, con alcance de carpetas que sí se aplica;
  `capabilities/shell/blocklist.ts`: lista de comandos prohibidos que no se puede anular, con variantes de PowerShell).
- **Qué verías:** una sola pregunta al crear el bot («¿cuánto confías en él?») que configura de verdad permisos, reglas y límites.
- **Cómo en Codync:** el perfil escribe `permission` y `approvalRules` coherentes (hoy es solo «preguntar» o «auto») y añade una lista fija de
  comandos que ni «auto» aprueba.
- **Hoy:** reglas por patrón en el host sin ninguna pantalla que las edite. **Tamaño:** M. **Código:** **A** para la lista de comandos (son datos), B para el resto.

### 2.10 Ahorro de tokens y roles de modelo
- **Fuente:** Coworkers (`BACKGROUND_MODEL`: turnos de fondo con un modelo más barato), Mercury (`core/saver-mode.ts`), Rakazo (`model-role-settings.ts`:
  qué modelo para chat, fondo y resumen).
- **Qué verías:** «Modo ahorro» por bot y un modelo propio para rutinas y check-ins; el presupuesto diario ya existente avisa y pausa.
- **Cómo en Codync:** ajuste por bot «modelo de fondo» y roles pasados al agente al crear la sesión; el vigilante de `budget.rs` ya cuenta los tokens.
- **Hoy:** presupuesto diario sin ahorro automático ni roles. **Tamaño:** M. **Código:** B.

### 2.11 Diario del bot y retención
- **Fuente:** Mercury (`bots/journal.ts`, `retention.ts`).
- **Qué verías:** una bitácora legible de lo que hizo cada bot cada día («hoy resumió 3 correos, falló una rutina») y una política para borrar lo antiguo.
- **Cómo en Codync:** el host escribe entradas del diario desde eventos del hub; los clientes la muestran junto a la memoria.
- **Hoy:** no existe. **Tamaño:** M. **Código:** B.

### 2.12 SDK de plugins y acompañantes en Rust
- **Fuente:** Mercury (`src/plugins/`: `sdk.ts`, `installer.ts`, `manager.ts`, `channel-bridge.ts`, `tool-bridge.ts`, `conformance.ts`;
  acompañantes en **Rust** `plugins/whatsapp/companion` —crate `mercury-wa`, MIT, sobre `whatsapp-rust`— y `plugins/optimus/companion`, de voz).
- **Qué verías:** instalar un canal o herramienta nueva desde el catálogo sin actualizar Codync; terceros podrían publicar los suyos.
- **Cómo en Codync:** definir un contrato de plugin (procesos aislados que hablan con el host) y usar los acompañantes en Rust como primeros plugins
  de WhatsApp y de voz. **Aviso:** el de WhatsApp usa un protocolo no oficial; hay riesgo de bloqueo de cuenta.
- **Hoy:** hay catálogo de conectores y habilidades; no hay SDK de plugins (sin resultados). **Tamaño:** XL. **Código:** **A** para evaluar los acompañantes (mismo lenguaje).

### 2.13 Correo propio para cada bot
- **Fuente:** Coworkers (`onEmail` en `bot-agent.ts`: dirección `usuario.bot@dominio`, guarda el mensaje en `/workspace/inbox/email/<fecha>-<asunto>` y ignora
  autorrespuestas).
- **Qué verías:** reenvías un correo al bot y lo procesa, o él te escribe desde su dirección.
- **Cómo en Codync:** el relay de Codync Cloud recibe el correo y lo entrega al host como el evento `email` que ya existe en las rutinas.
- **Hoy:** existe el evento `email`; no hay dirección por bot. **Tamaño:** L (requiere dominio y relay). **Código:** C.

### 2.14 Voz: voces propias y manos libres
- **Fuente:** Rakazo (proveedores ElevenLabs, OpenAI, Cartesia y Fish Audio con tu clave), Coucou («OK Coucou»: `ClaudeVoiceBrain.swift`, `IntentParser.swift`).
- **Qué verías:** eliges la voz de cada bot; dices «OK Codync, ¿cómo van mis bots?» sin tocar nada.
- **Cómo en Codync:** más proveedores TTS en `host/src/voice.rs` con claves en la bóveda; detección de palabra clave en el dispositivo para el manos libres.
- **Hoy:** sin ElevenLabs, Cartesia ni palabra de activación (sin resultados). **Tamaño:** M para las voces, L para el manos libres. **Código:** B.

### 2.15 Notas de reuniones sin bot
- **Fuente:** Coucou (`NoteTaker.swift`, `NoteAudio.swift`, `NoteEcho.swift`, `NoteModels.swift`; según su README: graba tu micrófono y lo que reproduce
  el Mac, sin que entre un bot a la llamada, y entrega transcripción por persona, resumen, puntos clave y pendientes).
- **Qué verías:** «Grabar reunión» y después un resumen con tareas que el bot puede crear como rutinas o elementos abiertos.
- **Cómo en Codync:** captura de audio del sistema en Windows (WASAPI loopback), transcripción y resumen por el agente; los pendientes pasan a `open_items`.
- **Hoy:** hay llamadas de voz con recibos; nada de reuniones. **Tamaño:** XL. **Código:** C.

---

## 3. Qué quedó fuera de esta sección (ya existe en Codync)

TipTap con comandos «/», tareas y tablas (`document-editor.ts`), documentos y revisión humana, `request_secret` con bóveda cifrada, búsqueda y ejecución de
herramientas de Composio, soltar archivos en la isla y aprobar desde ella, presupuesto diario, política de salida de red, canal de Slack, reglas de aprobación,
rutinas con vigilancia de archivos y URL, memoria con versiones, widgets, Live Activity, intents, hooks para sesiones externas y la pantalla remota.

---

## 4. Hooks: ideas grandes que faltan

**Punto de partida.** Codync ya normaliza `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PermissionRequest`, `Stop`, `Interrupt`,
`SubagentStart`, `SubagentStop` y los eventos de Gemini (`host/src/external/`). `hooks install` solo escribe en el `settings.json` de Claude Code.

### 4.1 Preguntas y planes de sesiones externas, contestables desde cualquier cliente
- **Fuente:** Coucou (`QuestionPayload.swift`, `QuestionCard.swift`: hasta 4 preguntas, selección única o múltiple, y el Mac solo acepta etiquetas que estén entre las
  opciones vigentes; aprobación del plan desde la isla en su 0.5.2).
- **Qué verías:** cuando Claude Code pregunta «¿cuál de estas tres?» o propone un plan, te llega una tarjeta con las opciones al teléfono o a la isla y tu
  respuesta vuelve a la terminal.
- **Cómo en Codync:** `external/permission.rs` hoy se salta `AskUserQuestion` (línea 68). Añadir un tipo de tarjeta de pregunta y devolver la
  elección por el hook; igual para `ExitPlanMode`.
- **Hoy:** no. **Tamaño:** M-L. **Código:** B.

### 4.2 Automatizaciones salientes de Codync («hooks de Codync»)
- **Qué verías:** «cuando un bot termine, una rutina falle o una aprobación quede pendiente, ejecuta este comando o llama a este webhook», con
  una lista editable y un registro de lo que se ejecutó. Conecta con n8n, scripts o tu domótica.
- **Cómo en Codync:** tabla de automatizaciones por bot en el host que se suscribe a los eventos del hub; la ejecución con tiempo límite y sin dar al
  script acceso a secretos.
- **Hoy:** no existe. **Tamaño:** M-L. **Código:** B.

### 4.3 Reglas, presupuesto y «hecho» sobre sesiones externas
- **Idea:** aplicar a tus sesiones de Claude Code y Codex lo que Codync ya sabe hacer con sus bots: `PreToolUse` consulta tus reglas de aprobación y la lista de
  comandos prohibidos; el presupuesto del día frena cuando se agota; un hook `Stop` impide terminar si las pruebas no pasan (la idea de Mercury, 2.8). Tu propio
  Codex ya usa un hook `Stop` («Design deep pass» de impeccable): el mecanismo funciona.
- **Cómo en Codync:** ampliar `external/permission.rs` para llamar a las reglas y a `budget.rs`, y añadir el hook `Stop` opcional por sesión.
- **Hoy:** las tarjetas de aprobación de sesiones externas existen; no encontré que apliquen las reglas de Codync. **Tamaño:** M-L. **Código:** B.

### 4.4 Contexto de Codync dentro de Claude Code y Codex
- **Idea:** al iniciar una sesión externa (`SessionStart`, `UserPromptSubmit`) el hook inyecta la memoria del bot, los elementos abiertos y las instrucciones; antes
  de que el agente compacte su contexto (`PreCompact`) el hook vuelca lo importante a la memoria de Codync. Es «la memoria de Codync dentro de tu terminal».
- **Aviso:** depende de lo que cada agente admita en la respuesta del hook; verificarlo en su documentación antes de empezar.
- **Hoy:** no existe. **Tamaño:** L. **Código:** B.

### 4.5 Instalador universal de hooks con vista previa y respaldo
- **Fuente:** Coucou (`ClaudeSettingsFile.swift`: nunca partir de un objeto vacío si el archivo existe pero no se puede leer, copia de respaldo y escribir solo
  sobre los mismos bytes que se mostraron en la vista previa; `windows/hook/src/normalize.rs`, en Rust y MIT, con los mapeos de Codex, Gemini, OpenCode, Amp y
  otros).
- **Qué verías:** `codync-host hooks install --agent codex` (y gemini, antigravity, copilot, opencode, amp) con vista previa del cambio y respaldo; hoy los
  que no son Claude se configuran a mano. En tu PC, `~/.codex/hooks.json` ya acepta `PostToolUse`, `Stop`, `SessionStart`, `UserPromptSubmit`,
  `PermissionRequest`, `SubagentStart/Stop` e `Interrupt`.
- **Hoy:** solo Claude Code. **Tamaño:** M. **Código:** **A** (Rust, MIT) para los mapeos.

### 4.6 Árbol de subagentes y «qué hizo mientras no estabas»
- **Fuente:** Coucou (`LastTurnView.swift`, `RecapStore.swift`), y los eventos `SubagentStart/Stop` que Codync ya recibe.
- **Qué verías:** los subagentes aparecen como hijos del bot en la lista y en la isla; al volver, un resumen del último turno con herramientas, archivos y comandos.
- **Hoy:** llegan los eventos; no hay árbol ni resumen de ausencia. **Tamaño:** M-L. **Código:** B.

### 4.7 Eventos de Git y del entorno como disparadores de rutinas
- **Idea:** un `post-commit` o `pre-push` que avisa al host y dispara una rutina («revisa mi commit», «actualiza el changelog»); lo mismo con el terminal o el
  editor abriendo un proyecto.
- **Hoy:** las rutinas tienen horario, webhook, eventos de servicios y vigilancia de archivos; no tienen eventos locales de Git. **Tamaño:** M. **Código:** B.

### 4.8 Sesiones de otras computadoras reportando a tu host
- **Idea:** el hook de un agente que corre en otra máquina (por SSH) publica en tu host a través del relay cifrado y su sesión aparece en tu lista.
- **Hoy:** el hook habla con `POST /ingest/hook` en loopback. **Tamaño:** L. **Código:** C.

---

## 5. CloudKit: ideas grandes (solo dispositivos Apple)

**Qué es hoy y qué sería.** Codync no usa CloudKit. El iPhone y el resto de clientes hablan con el host por el relay de Cloudflare con cifrado extremo a
extremo y reciben push por un ticket APNs. Coucou sí usa CloudKit (`NotchBuddy/Sources/App/PhoneLink/`: `SessionPublisher.swift`, `ApprovalRelay.swift`,
`LiveActivityRelay.swift`; `Phone/PhoneLink.swift`). **Límite:** funciona solo entre dispositivos Apple. Android y Windows no pueden usarlo de forma práctica, así
que sería un **modo opcional «solo Apple»** y no un reemplazo del relay.

### 5.1 Modo «solo iCloud»: sin relay ni cuenta propia
- **Qué verías:** Mac e iPhone con la misma cuenta de iCloud se sincronizan sin cuenta Codync ni servidor: un registro por sesión o bot en la base
  privada del usuario, en una zona propia.
- **Reparto de campos (de `SessionPublisher.swift`):** en claro lo necesario para ordenar y para widgets (id, estado, conteos de pasos, banderas, fechas); cifrados con
  las claves de iCloud el nombre del proyecto, los pasos, la carpeta, el último mensaje, el comando pendiente y la pregunta.
- **Tamaño:** XL. **Código:** B (Swift, MIT).

### 5.2 Aprobaciones sin servidor de push propio
- **Fuente:** `ApprovalRelay.swift`: se escribe un registro `ApprovalRequest`; una suscripción por consulta lo convierte en notificación en el iPhone; el Mac solo
  busca el registro `Decision` (cada 2 s) **mientras hay una petición pendiente** y lo aplica únicamente si la huella coincide (sesión, herramienta, comando y entrada); una
  decisión tardía o para otro comando se ignora. El registro se borra al resolverse en cualquiera de los dos lados. Solo «permitir» y «denegar»: nunca «siempre» desde el teléfono.
- **Cómo en Codync:** la huella ya existe («clave del efecto»); lo nuevo es evitar la clave APNs y el relay de push para las aprobaciones en Apple. **Tamaño:** L.

### 5.3 Emparejamiento automático por cuenta de iCloud
- **Qué verías:** instalas Codync en el iPhone y aparece tu Mac sin código ni QR, porque comparten cuenta. **Tamaño:** M. **Hoy:** el emparejamiento es por QR o enlace.

### 5.4 Compartir un bot o una sesión con otra persona
- **Idea:** invitar a alguien a ver (no escribir) una sesión, un tablero o una mini-app usando el compartir entre cuentas de CloudKit. **Tamaño:** XL.
  Idea de producto grande, sin equivalente hoy.

### 5.5 Handoff y Spotlight entre Mac e iPhone
- **Qué verías:** empiezas un chat en el Mac y lo continúas en el iPhone en el mismo punto; buscas en Spotlight lo que dijo un bot hace semanas
  (`SearchAndSpotlight.swift`). **Tamaño:** M-L.

### 5.6 Respaldo y restauración de bots en iCloud
- **Qué verías:** «Respaldar en iCloud» exporta el paquete del bot (ya existe `host/src/bundle.rs`) y lo restauras en otro Mac. **Tamaño:** M.

### 5.7 Widgets más listos con tono y urgencia
- **Fuente:** `SharedSessions.swift`: cada sesión lleva `tone` (waiting, question, error, working, done, idle, warning, info), `urgency` (menor = más urgente),
  `stepIndex/stepCount` y `currentStep`.
- **Cómo en Codync:** añadir `tone` y `urgency` a `WidgetFeed` para ordenar los widgets de Android e iOS con la misma regla; encaja con el prompt de widgets v2. **Tamaño:** M.

### 5.8 Sincronizar ajustes entre tus dispositivos Apple
- **Qué verías:** apariencia, orden de bots y filtros iguales en todos tus dispositivos (almacén clave-valor de iCloud). **Tamaño:** M.

**Antes de empezar con CloudKit:** crear el contenedor y el esquema por entornos (desarrollo y producción) y decidir qué campos van en claro; la base privada
consume la cuota de iCloud del propio usuario.

---

## 6. Qué haría primero (por impacto y esfuerzo)

| Orden | Idea | Tamaño | Por qué |
|---|---|---|---|
| 1 | 4.1 Preguntas y planes de sesiones externas | M-L | Hoy se pierden en la terminal; es el hueco más claro en lo que ya funciona |
| 2 | 2.1 Telegram como segundo canal | L | Es lo que más amplía el alcance; hay plantilla en `slack/` |
| 3 | 2.9 + 2.10 Perfiles de seguridad y ahorro de tokens | M | Dos pantallas de ajustes que dan control real |
| 4 | 2.8 Fin de turno honesto | M-L | Cambia la confianza en los bots de código |
| 5 | 1.4 Panel de salud y gobernador | M-L | Con 8 GB es una necesidad real |
| 6 | 2.3 Importar agentes | M-L | Código reutilizable casi tal cual; baja la barrera de entrada |
| 7 | 2.6 + 2.7 Habilidades propuestas y «lo que sé de ti» | L | Hacen que la memoria y las habilidades crezcan solas |
| 8 | 1.7 + 1.8 «Tu semana» y modo demo | M | Retención y primer minuto |
| 9 | 2.2 Tuberías con aprobación humana | L | Encaja con las rutinas por nodos ya planificadas |
| 10 | 1.1 Kanban + 1.2 Cabina web | L + XL | Los dos cambios de interfaz más visibles |
| 11 | 4.2 + 4.3 Hooks salientes y reglas en sesiones externas | M-L | Aprovechan lo que ya existe |
| 12 | 1.3 Gráficas | M-L | Mejora todas las respuestas con datos |

## 7. Lo que se quitó de la lista anterior

- Todo lo que ya existe en Codync (sección 3).
- Las mejoras pequeñas: aviso a las 4 h y cierre a las 24 h de una aprobación sin respuesta, separador de mensajes nuevos, fechas entre mensajes, insignia de
  no leídos, consentimiento de IA, `SETUP_PROMPT.md`, atajos configurables, sonidos propios, tema oscuro derivado, comandos rápidos sin modelo y similares. Siguen en
  `analisis-ideas-6-repos-2026-10-28.md` y en el historial de git (commit `35450a68`, con el nombre `ideas-30-por-repo-como-integrarlas-2026-10-28.md`) por si quieres rescatarlas.
- Lo que no se puede o no conviene reutilizar: el arte de Coucou (Mochi, trajes y sonidos) y los núcleos que dependen de Cloudflare o Postgres.

## 8. Límites

- No ejecuté ni compilé el código de ninguno de los cinco repos. Cuando un punto viene solo del README de un repo, lo digo en la ficha.
- «No existe» significa que no lo encontré con `git grep` en el host, el escritorio, Android, iOS y `docs/features`; puede haber algo con otro nombre.
- Las funciones de Coucou en Android y sus widgets los vi por archivos y por su README, no los instalé.
