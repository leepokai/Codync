# Entrega de la integración Hermes

El trabajo está en `feat/hermes-integration`, en el worktree
`C:\Users\Edwin Villeda\Documents\hermes-integration`.
Se crearon diez commits por las fichas H1–H10 y un cierre de entrega.
La integración compartida se conserva para la autorización «llévalo a dev».

El clon de referencia permanece limpio en
`10d25f8ad65fdf71304e9272959421a0c22e5d23`. Las piezas adaptadas conservan
sus nombres internos y permisos, con atribución MIT. Los clientes envían
intenciones al host y dibujan su estado.

## Integración en código

- **H1** usa el cliente ACP del host y el proveedor Hermes existente. El instalador
  fija la revisión y respeta el entorno Python propio. Las aclaraciones de Hermes
  conservan sus opciones y requieren respuesta manual. La cancelación del host
  llega al turno ACP y el proceso tiene propietario y cierre acotado.
- **H2** incluye 185 habilidades completas y 1.281 archivos auxiliares del catálogo
  Hermes. Reutiliza los bundles y el índice de Codync. Las mejoras crean versiones
  por bot, permiten restauración con revisión esperada y conservan las licencias.
  La creación de habilidades reutiliza el flujo existente de propuesta y revisión.
- **H3** incorpora búsqueda FTS con intervalos de fechas sobre el historial público
  del bot. Reutiliza la memoria persistente y Dreamer de Codync. El ejecutor nativo
  permite seleccionar memoria del perfil, builtin o Honcho. Honcho conserva su
  plugin y credenciales existentes y comunica los errores de inicialización.
- **H4** conecta Signal, Email y Home Assistant mediante transportes nativos,
  configuración de Apps existente, sesiones por conexión y conversaciones propias.
  El host controla pairing, autorizaciones, generaciones, aprobaciones, inbox/outbox,
  adjuntos y transcripción. Email admite un webhook firmado. Los adjuntos salientes
  se resuelven desde las publicaciones del bot y se verifican por tamaño y SHA256.
  La configuración genérica permite el código inicial y la política de remitentes.
  Las operaciones adicionales de estado y revocación individual se exponen por API.
- **H5** importa los 16 blueprints de cron y los adapta al scheduler de rutinas.
  La entrega usa una cola persistente por corrida y destino, con autorización
  capturada y recibos de transporte. Incluye WhatsApp con su conexión existente.
  Los cuatro clientes editan destinos con control de cambios simultáneos.
- **H6** ejecuta local, Docker, SSH, Modal, Daytona y Vercel Sandbox mediante las
  fábricas nativas y la política administrada de Hermes. SSH sincroniza el proyecto
  en ambos sentidos y controla sus procesos. Los cuatro clientes permiten elegir
  el entorno y pedir una comprobación explícita de salud de la revisión guardada.
- **H7** ofrece rutas guardadas de proveedor/modelo por tarea, presupuestos de
  iteraciones e inactividad, y cupos compartidos entre conversaciones y rutinas
  del bot. Las credenciales se resuelven en Hermes. La delegación entrega el resumen
  al padre. La CLI RPC reutiliza las herramientas y la conexión del host.
- **H8** incorpora NeuTTS 1.4.1 con inferencia offline progresiva y cancelación por
  solicitante. Desktop, iOS y Android conservan sus conexiones de transcripción.
  La frase móvil Hey Hermes usa reconocimiento nativo local, está desactivada
  inicialmente y funciona durante la conversación en primer plano.
- **H9** expone bots mediante los endpoints locales OpenAI compatibles de modelos,
  conversación, salud y precios. Las claves se administran mediante el vault.
  Las solicitudes pasan por la cola del bot y conservan herramientas y aprobaciones.
- **H10** importa 66 manifiestos MCP, evita duplicados con OpenBot y conserva los
  servidores existentes. Los flujos OAuth reutilizan PKCE y vault. El catálogo
  muestra origen y estado y conecta las piezas comunes del host y sus clientes.

## Procedencia y puentes

- ACP y ejecutor proceden de `acp_adapter/{entry,server,session,elicitation}.py`.
  La adaptación vive en `host/src/agent/hermes/` y en el actor ACP. Se reutilizan
  helpers de perfil y logging para evitar el arranque del actualizador nativo.
- Las habilidades se extraen de `skills/` y `optional-skills/`. Los paths completos
  están en `host/src/market/hermes/skills.json` y el archivo conserva sus auxiliares.
- Memoria y sesiones toman como referencia `agent/memory_manager*.py` y
  `agent/runtime_session_store.py`. La persistencia del host sigue siendo la de Codync.
- Signal y Email reutilizan `plugins/platforms/signal/` y
  `plugins/platforms/email/`, con los contratos de `gateway/platforms/`.
- Home Assistant usa `homeassistant/{adapter,tools}.py` y su licencia del plugin
  oficial fijado en `e7a218f26db88426ab3e7376f0e000b6e356f5e7`.
- Rutinas toman `cron/blueprint_catalog.py`, `delivery_queue.py` y
  `delivery_outcome.py`. Los 16 esquemas se conservan en `blueprints.json`.
- Los entornos reutilizan `tools/terminal_scope.py`, `terminal_tool_backends.py`,
  `tools/environments/` y `tools/process_registry.py`. La adaptación SSH limita la
  sincronización al proyecto y conserva handles cancelables para trabajos de fondo.
- Las rutas de subagentes usan el contrato de `agent/delegation_provider.py`,
  `plugins/delegation/default/` y `tools/delegate_tool*.py`.
- Voz reutiliza `tools/neutts_synth.py`, `tools/neutts_samples/jo.*` y el extra
  NeuTTS fijado en `pyproject.toml`. Los frames proceden de `infer_stream`.
- La API local toma como referencia `gateway/platforms/api_server_openai_routes.py`.
  El inventario MCP conserva los orígenes de `optional-mcps/*/manifest.yaml`.

Los detalles de cada adaptación, sus diferencias y sus contratos están en
`docs/architecture/hermes-integration.md`,
`docs/integrations/hermes-h4-native-channels.md` y
`docs/integrations/hermes-h8-voice.md`.

## Revisión y comprobaciones

Tres revisiones independientes examinaron corrección, completitud y refactorización.
Se corrigieron el import que podía modificar el clon fijado, opciones heredadas de
otro proveedor, conflictos de sincronización SSH, resolución y limpieza de adjuntos,
la acción de salud del backend y respuestas o avisos de otra computadora en iOS.
También se añadieron regresiones para esos casos.

Se analizaron 141 archivos Rust con rustfmt, 14 fuentes TypeScript con el parser
Babel y las fuentes Python y scripts remotos mediante AST. La revisión final de
espacios usa el diff completo desde la base de la tarea.

La instrucción del usuario de continuar sin compilar sigue vigente. Las pruebas
añadidas permanecen sin ejecutar. Quedan pendientes cargo/clippy/tests, typecheck,
las pruebas móviles, `hermes doctor` del ejecutor y los recorridos en Codync Dev.
La disponibilidad de servicios, credenciales, modelos, permisos de micrófono,
latencia y reproducción/interrupción requiere esas comprobaciones reales.
La entrega de código no acredita los criterios de aceptación ejecutados de H1–H10.

## Límites de la adaptación

- SSH requiere Python 3 y POSIX `waitid(WNOWAIT)` en el destino. Se sincronizan hasta
  10.000 archivos, 64 MiB por archivo y 128 MiB por proyecto. Se excluyen enlaces,
  credenciales y cachés. Se comparan los cambios locales antes de publicar cada
  archivo. El filesystem conserva una ventana breve entre comparar y reemplazar.
- La pérdida de conexión usa un lease de 20 segundos para terminar procesos SSH.
  La recuperación de archivos depende de que el destino vuelva a estar disponible.
  Los escapes deliberados de grupos de procesos requieren contención del sistema.
- Los backends serverless conservan sus políticas y costes nativos. La salud es una
  acción explícita y puede crear un sandbox. Un timeout informa limpieza no confirmada.
- Los cupos de subagentes tienen un máximo de diez. Los hijos anidados rechazan un
  pool lleno para evitar que el padre bloquee todos los cupos esperando nietos.
- NeuTTS requiere su entorno Python 3.11–3.13 y modelos preparados. Hey Hermes se
  adapta con reconocimiento del sistema móvil. TUI conserva conversación de texto.
- Los envíos ambiguos conservan un recibo y requieren revisión humana para repetir.
  Los precios de proveedor desconocidos se devuelven como null.

Como mejoras adicionales se incorporaron control de ediciones simultáneas,
aislamiento de conversaciones de mensajería, limpieza de snapshots, cupos entre
procesos, protección al cambiar de computadora y comprobaciones de salud explícitas.
