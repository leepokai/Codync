# Qué podemos saber de seis repos: ideas para Codync (2026-10-28)

Repos revisados: Coworkers/bdots, OpenDots, Rakazo, Coucou, Mercury Agent y el repo
`fayazara/coworkers` (que es el mismo que `fayazara/bdots`, ver ficha 1).

Este documento contesta tres preguntas: qué hace cada repo, qué de eso ya tiene Codync y qué
falta de verdad. La última parte es el trabajo pendiente ordenado.

## 1. Resumen y una corrección importante

- **Esta versión solo conserva lo parcial (🟡) y lo nuevo (❌).** Se quitaron las ideas que Codync ya
  tiene (presupuesto diario, política de salida de red, isla, canal de Slack, reglas de aprobación,
  rutinas con vigilancia, memoria con versiones, documentos con revisión, widgets y hooks de sesiones
  externas, entre otras) y se verificaron las que antes figuraban como «no comprobadas».
- **Una recomendación mía anterior era errónea.** Dije que había que «aplicar reglas por tipo de
  acción», «añadir presupuesto por bot» y «una pasarela a Slack» como pendientes. Las dos últimas ya
  existen y la primera es parcial (sección 5.1). Las tablas de abajo reemplazan esa lista.
- **Lo que de verdad falta** (verificado, sección 5): recordatorio y cierre de aprobaciones sin
  respuesta, modelo barato para turnos de fondo, modo demo, resumen semanal, personalidad en
  archivos, «hecho solo si pasan las pruebas», canales Telegram/Discord/WhatsApp/Signal, catálogo
  Pipedream, tarjetas de servicios, aprendizaje automático de habilidades y computadoras en la nube
  por bot.

## 2. Cómo se hizo y límites

- **Fuentes:** el README de cada repo (leído hoy con `gh api`) y los datos públicos de GitHub
  (estrellas, último push, licencia). **Esta primera versión no abrió el código fuente**; la segunda parte,
  `ideas-nuevas-y-grandes-2026-10-28.md`, sí lo leyó (clones en `Documents\ref-repos`) y corrige
  lo de las licencias de abajo.
- **Estado de Codync:** comprobado con `git grep` y lectura de archivos en la rama
  `integrate/windows-all-additions` (Rust del host, escritorio, Android, iOS y `docs/features`). Un
  «no existe» significa que no lo encontré en esas rutas, no que sea imposible que exista.
- **No probado en ejecución:** nada de lo marcado «Ya existe» lo probé corriendo, salvo lo de las
  mini-apps y los check-ins que sí se probó el 2026-10-26. Las marcas indican que el código existe.
- Leyenda: **🟡** parcial · **❌** falta. Lo que ya existe (✅) se quitó.

## 3. Fichas de los repos

| # | Repo | Qué es | Pila | Licencia declarada | Estrellas · último push |
|---|---|---|---|---|---|
| 1 | `fayazara/bdots` = `fayazara/coworkers` | «Compañeros» de IA, cada uno con su computadora y navegador, todo sobre Cloudflare. Experimental. El README se titula *Coworkers*; la URL `bdots` es el nombre anterior (misma descripción y estrellas) | TypeScript, Workers, Durable Objects, SQLite por bot, Browser Run | MIT (archivo `LICENSE`, Fayaz Ahmed) | 319 · 2026-10-27 |
| 2 | `CopilotKit/OpenDots` | Plantilla de agentes persistentes con espacios de documentos, llamadas y Slack. Alfa | TypeScript, CopilotKit, AG-UI, TanStack AI | MIT (archivo `LICENSE`) | 6 162 · 2026-10-28 |
| 3 | `elie222/rakazo` | Alternativa abierta a Grok Bot: bots persistentes con memoria y rutinas, computadoras privadas o compartidas, voz. Web, Electron, iPhone y Android. Beta | TypeScript, React 19, Electron, Expo, Hono, PostgreSQL | Apache-2.0 (archivo `LICENSE`) | 6 161 · 2026-10-29 |
| 4 | `Louis-CFM/coucou` | Una mascota (Mochi) en la «isla» de la pantalla que vigila agentes de código y deja aprobar desde ahí o desde el teléfono; además dictado y notas de llamadas | Swift 6/SwiftUI, Tauri 2 (Windows/Linux), Android por un colaborador | Código MIT (archivo `LICENSE`); el nombre, Mochi y los sonidos son © del autor (`LICENSE-ASSETS.md`) | 6 685 · 2026-10-23 |
| 5 | `cosmicstack-labs/mercury-agent` | Agente con personalidad editable, memoria local, permisos estrictos, presupuesto de tokens y bots especialistas. Terminal, Telegram, Discord, Slack, Signal y panel web | TypeScript, SQLite con FTS5 | MIT | 3 683 · 2026-10-28 |

## 4. Matriz de ideas: solo lo parcial y lo nuevo

### 4.1 Aprobaciones y seguridad

| Idea | Fuente | Estado en Codync | Viabilidad / esfuerzo |
|---|---|---|---|
| Recordatorio push a las 4 h de una aprobación sin responder y cierre a las 24 h | Coworkers | ❌ Solo hay `expire_abandoned` (cierra la tarjeta cuando el agente deja de esperar) | Alta / bajo |
| Botón «Siempre» que crea una regla persistente | Coucou (Allow / Deny / Always) | 🟡 La tarjeta ya ofrece `allow_always` cuando el agente lo da, pero Codync no lo convierte en una regla suya | Alta / bajo |
| Editar `approvalRules` desde los clientes | Rakazo | ❌ «Ningún cliente las edita todavía» | Alta / medio |
| Reglas por clase de acción (`actionRules`: archivos, comandos, web, navegador, apps, bots, rutinas) | Coworkers | 🟡 Se guardan (`store/model.rs`) pero nada las aplica. Decidir si se aplican o se fusionan con `approvalRules` | Alta / medio |
| Defecto seguro en segundo plano: solo herramientas de lectura cuando nadie puede aprobar | Coworkers | ❌ No existe | Alta / bajo |
| Anti-bucle de llamadas repetidas | Coworkers | 🟡 `stall.rs` cubre llamadas que fallan igual; no vi cobertura de repeticiones que sí tienen éxito | Media / bajo |
| Revisión automática de peticiones de bajo riesgo por un modelo | Rakazo | ❌ Decisión consciente de no hacerlo (`approvals.md`) | Baja, riesgo alto |
| Permisos de lectura/escritura por carpeta, listas negras de shell no anulables, ocultar secretos en logs | Mercury | 🟡 Hay reglas por patrón (`approvalRules`); no hay lista negra fija de shell | Media / medio |
| Seguridad a nivel de fila en la base de datos | Rakazo | No aplica: Codync usa SQLite local de una persona | — |

### 4.2 Motor de bots, rutinas y memoria

| Idea | Fuente | Estado | Viabilidad / esfuerzo |
|---|---|---|---|
| Modelo más barato para turnos de fondo (rutinas, check-ins) y «Token Saver» | Coworkers (`BACKGROUND_MODEL`), Mercury | ❌ No encontré nada | Alta / bajo-medio |
| Presupuesto por flota (equipo) además de por bot | Mercury | 🟡 Hay presupuesto por bot o global; no por equipo | Media / bajo |
| Personalidad en archivos Markdown editables (`soul`, `persona`, `taste`, `heartbeat`) | Mercury | ❌ Los bots tienen instrucciones, no esos archivos | Alta / bajo |
| «Hecho» solo si una compilación, prueba o typecheck pasó | Mercury Code | ❌ | Media / medio (política por bot) |
| Memoria con 10 tipos de dato, confianza y caducidad automática | Mercury | 🟡 Hay memoria en archivos con dreaming, sin tipos ni confianza | Media / alto |
| Más formas de despertar un bot: edición de una mini-app compartida, otro bot | Coworkers | 🟡 Hay webhook, eventos, mensajes entre bots; no «edición de mini-app» | Media / medio |
| Flotas: líder que delega tareas tipadas, tuberías en `bot.yaml`, entregables en una carpeta | Mercury | 🟡 Hay equipos con coordinador; no tuberías ni carpeta de entregables | Media / alto. Se cruza con rutinas por nodos |
| Rutinas por nodos con pasos y estado | Openstark | ❌ Ver prompts de rutinas por nodos | Media / alto |
| Aprendizaje automático: el bot propone habilidades a partir de conversaciones y tú apruebas antes de publicarlas | OpenDots | ❌ | Baja / alto |
| Enseñar por demostración grabando el navegador | Coworkers | 🟡 Existe `teach.rs`; no revisé su alcance | ? |

### 4.3 Interfaz y experiencia

| Idea | Fuente | Estado | Viabilidad / esfuerzo |
|---|---|---|---|
| Modo demo sin API ni configuración, que no escribe en disco | Coucou | ❌ | Alta / medio |
| Resumen semanal los lunes (tiempo, sesiones, archivos, permisos) | Coucou | ❌ | Alta / medio |
| «Novedades» tras actualizar y «tu Codync hasta ahora» | Coucou | ❌ | Media / bajo |
| Mascota viva (ojos que siguen el cursor, emociones, sonidos, ropa) | Coucou | 🟡 Hay personajes de bot y animaciones, sin esa personalidad | Media / medio |
| Isla en Android (recorte de cámara), icono de ajustes rápidos, notificación fija | Coucou Android | 🟡 En Android hay superficies de estado; no comprobé la isla | Media / medio |
| Widgets por color de estado, widget de equipo, botones rápidos | Coucou Android (9 widgets) | 🟡 Hay 5 widgets; ver prompt de widgets v2 | Alta / medio |
| Preguntas del agente con varias opciones desde la notificación | Coucou | ❌ Solo `external/permission.rs` menciona `AskUserQuestion`, y lo omite | Media / medio |
| Control Center, filtro de Focus, Spotlight y Siri en iOS | Coucou iPhone | 🟡 Hay intents; no encontré Control Center ni Spotlight ni Focus | Media / medio, solo con Mac |
| Tema claro, colores del teléfono, más idiomas (14, con árabe) | Coucou Android | 🟡 Hay 7 idiomas (de, es, fr, ja, ko, zh-Hans, zh-Hant) frente a 14 | Media / bajo |
| Panel web con Kanban, explorador de memoria y cabina de bots | Mercury | ❌ | Baja / alto |
| Adjuntar una ventana o la pantalla al chat | Coucou | ❌ | Media / medio (Electron lo permite) |
| Notas de llamadas sin bot, con transcripción y resumen | Coucou | ❌ Hay recibos de llamada de voz, no notas de reuniones | Media / alto |
| Llamada con un agente de cómputo aparte para que la charla siga | OpenDots | ❌ No existe | Media / alto |

### 4.4 Canales e integraciones

| Idea | Fuente | Estado | Viabilidad / esfuerzo |
|---|---|---|---|
| Telegram, con códigos de emparejamiento y botones para aprobar | Mercury | ❌ | Media-alta / medio. Copiar el patrón de `host/src/slack/` |
| Discord | Mercury | ❌ | Media / medio |
| WhatsApp (por pasarela) | Coucou, Mercury | ❌ | Baja-media / alto, depende de terceros |
| Signal | Mercury | ❌ | Baja / alto |
| Catálogo Pipedream, MCP remoto propio y OpenAPI como fuente de herramientas | Rakazo | ❌ Pipedream; MCP sí | Media / medio. Treg se cobra por uso |
| Tarjetas de servicios (GitHub PRs y CI, Vercel, Stripe, Cal.com, n8n, Notion) con acciones seguras | Coucou | ❌ | Media / medio |
| Correo entrante por bot (`usuario.bot@dominio`) | Coworkers | 🟡 Existe el evento `email` en rutinas; no una dirección propia por bot | Baja / alto |
| Archivo `SETUP_PROMPT.md` para que un agente instale y configure | Rakazo | ❌ | Alta / muy bajo |
| Instalación de un comando con Docker | Rakazo | No aplica a una app de escritorio | — |
| Dictado global manteniendo una tecla, fuera de línea | Coucou | 🟡 Hay dictado y doble toque de ⌘ en escritorio; no comprobé mantener tecla ni motor fuera de línea en Windows | Media / medio |

### 4.5 Infraestructura

| Idea | Fuente | Estado | Viabilidad / esfuerzo |
|---|---|---|---|
| Computadoras en la nube por bot (Docker, E2B, Daytona, CreateOS, Cloudflare), privadas o compartidas por el equipo | Rakazo, Coworkers, OpenDots | ❌ | Baja. Codync apunta a una sola computadora local |

## 5. Trabajo pendiente ordenado

### Bloque A: barato y con lo que ya hay (primero)

1. **Aprobaciones completas.** Recordatorio a las 4 h y cierre a las 24 h; «Siempre» como regla
   guardada; pantalla para editar `approvalRules` en los cuatro clientes; decidir qué hacer con
   `actionRules` (aplicarlas o retirarlas para no tener dos sistemas).
2. **Modelo barato para el fondo.** Ajuste por bot para rutinas y check-ins; pasa por el
   presupuesto existente de `budget.rs`.
3. **Personalidad en archivos.** `soul`, `persona`, `heartbeat` como capas de instrucciones del bot,
   editables y con historial como la memoria.
4. **Modo demo y resumen semanal.** Dos funciones de onboarding y retención que usan datos locales.
5. **`SETUP_PROMPT.md`** para instalar y configurar Codync con un agente.
6. **Probar el canal de Slack** con un workspace real, antes de construir más canales sobre él.

### Bloque B: crecimiento

7. **Canales nuevos** reutilizando `host/src/slack/` como plantilla: Telegram primero, luego
   Discord. Cada uno con emparejamiento por código, lista de permitidos y aprobaciones por botones.
8. **Tarjetas de servicios** (GitHub y Vercel primero) con acciones seguras.
9. **«Hecho» con verificación** como política por bot.
10. **iOS:** Control Center, Spotlight y filtro de Focus (requiere Mac).
11. **Adjuntar ventana o pantalla** al chat en escritorio.
12. **Rutinas por nodos** y **widgets v2**, ya con prompt propio.

### Bloque C: apuestas grandes

13. Aprendizaje automático de habilidades con revisión humana.
14. Notas de reuniones sin bot.
15. Computadoras en la nube por bot como opción además de la local.
16. Panel web con Kanban y cabina de bots.

## 6. Ideas grandes aunque no sean viables hoy

- **Mercado de habilidades y bots** instalables por otros usuarios.
- **Bots con su propia dirección de correo** y buzón.
- **Mascota que acompaña en el escritorio** y reacciona a las alertas (Coucou la saca de la isla).
- **Computadora privada frente a computadora compartida del equipo** como concepto de producto (Rakazo).
- **Portafolio, música y otras «pastillas»** de Coucou: curiosas, fuera del foco de Codync.

## 7. Licencias y reglas de uso

- **Corrección:** una versión anterior de este documento decía que no había licencia y que había que usar
  solo las ideas. Los cinco repos tienen archivo `LICENSE`: **MIT** (Coworkers/bdots, OpenDots, Coucou y
  Mercury) y **Apache-2.0** (Rakazo). El código se puede reutilizar con sus avisos.
- **Qué exige cada una:** MIT, conservar el aviso de copyright y la licencia. Apache-2.0, además conservar
  el `NOTICE` y marcar los archivos modificados.
- **Reservado:** el nombre, Mochi, el arte y los sonidos de Coucou (`LICENSE-ASSETS.md`).
- **Ya aplicado:** `budget.rs` y `egress.rs` dicen «idea de…; sin copiar código». Para código copiado, dejar el
  origen en la cabecera y en un archivo de atribuciones de terceros.
- **Rakazo y Treg:** Treg se cobra por uso y exige contrato escrito para reventa alojada.
- Las reglas completas y el veredicto por repo están en `ideas-nuevas-y-grandes-2026-10-28.md`.

## 8. Relación con el trabajo en curso

| Trabajo | Efecto |
|---|---|
| Fusión con v2.29.0 | Debe terminar antes de empezar el Bloque A, porque toca los mismos archivos del host |
| Widgets v2 de Android | Cubre las filas de widgets, ajustes rápidos y botones de 5.3 |
| Rutinas por nodos (referencia Openstark) | Se cruza con «flotas» de 5.2 |
| Revisión del commit `b6bb8228` | Hay que arreglar antes el mensaje visible de los check-ins y la navegación externa del WebView de mini-apps |

## 9. Cómo repetir la verificación

```
git grep -il -E "<patrón>" -- host/src apps/desktop/src apps/android/app/src/main apps/ios docs/features
```

Rutas leídas para este documento: `host/src/budget.rs`, `host/src/egress.rs`,
`host/src/agent/approval_rules.rs`, `host/src/agent/bot/stall.rs`, `host/src/routines/mcp.rs`,
`docs/features/{approvals,notch-island,slack-channel,mcp-tool-policies,connector-credentials,opendots-integration,opendots-extras,computer-use,context-and-memory}.md`
y `docs/design/bot-memory.md`.

## 10. Enlaces

- https://github.com/fayazara/bdots (el mismo repo que https://github.com/fayazara/coworkers)
- https://github.com/CopilotKit/OpenDots
- https://github.com/elie222/rakazo
- https://github.com/Louis-CFM/coucou
- https://github.com/cosmicstack-labs/mercury-agent
