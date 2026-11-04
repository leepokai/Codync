# Prompts para los agentes (2026-10-29)

Cada archivo de esta carpeta es un prompt completo y autosuficiente: se lo pegas a un agente y trabaja. **Un agente por prompt.** No hace falta dar los documentos de ideas ni la guía: cada prompt ya dice qué leer.

## Estado de partida (comprobado el 2026-10-29)
- `integrate/upstream-2.29` (9a23b386) ya trae v2.29.0 con mis miniapps, check-ins y decisiones pendientes.
- `upstream/main` está 66 commits por delante de v2.29.0 (2099c8a4): por eso el **prompt 00** va primero.
- Ya existen, sin fusionar en una sola rama: `feat/routine-flows-host`, `feat/routine-flows-ui` (escritorio y Android) y `feat/android-widgets-v2`. El **prompt 01** las junta.
- Mis documentos de ideas y la guía están en `integrate/windows-all-additions` (20a9d74a); los prompts los leen desde `C:\Users\Edwin Villeda\Documents\Codync\docs\nuevo thndr\` (solo lectura, tu copia local) y el prompt 00 los trae a la rama base.
- Los clones de referencia están en `C:\Users\Edwin Villeda\Documents\ref-repos\` (solo lectura).

## Orden y paralelismo

### Ola 0: base común (en orden)

| Prompt | Qué hace | Depende de | Rama |
|---|---|---|---|
| [00](00-sync-upstream.md) | Sincronizar con las 66 novedades de upstream | ninguna (va primero) | `integrate/upstream-sync` |
| [01](01-integrar-ramas-hechas.md) | Integrar las ramas de funciones ya hechas | 00 | `integrate/features-1` |

### Ola 1: independientes (pueden correr a la vez en agentes distintos)

| Prompt | Qué hace | Depende de | Rama |
|---|---|---|---|
| [02](02-ios-de-lo-hecho.md) | iOS de las rutinas por nodos y del widget | 01 | `feat/ios-features-1` |
| [03](03-fix-checkins-y-webview.md) | Arreglos del commit de miniapps y check-ins | 00 | `fix/checkins-and-webview` |
| [10](10-aprobaciones-atascadas.md) | Aprobación sin respuesta: aviso, cierre y toma de control | 00 | `feat/approvals-stuck` |
| [11](11-comandos-prohibidos.md) | Lista fija de comandos prohibidos | 00 | `feat/shell-blocklist` |
| [12](12-veredicto-fin-de-turno.md) | Fin de turno honesto: terminó, pausado o cortado | 00 | `feat/turn-verdict` |
| [14](14-preguntas-externas-host.md) | Preguntas y planes de sesiones externas: lado del host | 00 | `feat/external-questions-host` |
| [16](16-canales-contrato-slack.md) | Canales: probar Slack, contrato de canal y Socket Mode | 00 | `feat/channels-contract` |
| [19](19-ahorro-tokens-modelo-fondo.md) | Ahorro de tokens y modelo de fondo | 00 | `feat/token-saver` |
| [20](20-salud-host.md) | Salud de la computadora: host y gobernador | 00 | `feat/host-health` |
| [22](22-importar-agentes.md) | Importar agentes de otras herramientas | 00 | `feat/agent-import` |
| [23](23-resumen-semanal.md) | Resumen semanal «Tu semana con Codync» | 00 | `feat/weekly-recap` |
| [24](24-modo-demo.md) | Modo demo guiado | 00 | `feat/demo-mode` |

### Ola 2: dependen de un prompt de la ola 1

| Prompt | Qué hace | Depende de | Rama |
|---|---|---|---|
| [13](13-puerta-verificacion.md) | «Hecho» solo con evidencia de pruebas | 12 | `feat/verify-gate` |
| [15](15-preguntas-externas-clientes.md) | Preguntas y planes de sesiones externas: pantallas | 14 | `feat/external-questions-ui` |
| [17](17-canal-telegram.md) | Canal de Telegram | 16 | `feat/channel-telegram` |
| [21](21-salud-pantallas.md) | Salud de la computadora: pantallas | 20 | `feat/health-ui` |
| [30](30-editor-reglas.md) | Editor de reglas de aprobación | 11 (recomendado) | `feat/approval-rules-editor` |
| [34](34-memoria-creencias-host.md) | «Lo que sé de ti»: memoria como creencias (host) | 00 | `feat/memory-beliefs-host` |
| [36](36-habilidades-propuestas-host.md) | Bandeja de habilidades propuestas (host) | 00 | `feat/skill-proposals-host` |

### Ola 3: dependen de la ola 2

| Prompt | Qué hace | Depende de | Rama |
|---|---|---|---|
| [18](18-pantalla-canales.md) | Pantalla «Canales» en los cuatro clientes | 16, 17 | `feat/channels-ui` |
| [31](31-siempre-como-regla.md) | «Siempre» como regla guardada | 30 | `feat/always-rule` |
| [32](32-actionrules-decision.md) | Resolver `actionRules` frente a las reglas por patrón | 30 | `feat/action-rules` |
| [33](33-perfiles-seguridad.md) | Perfiles de seguridad al crear un bot | 11, 30 | `feat/security-profiles` |
| [35](35-memoria-creencias-pantallas.md) | «Lo que sé de ti»: pantallas | 34 | `feat/memory-beliefs-ui` |
| [37](37-habilidades-propuestas-pantallas.md) | Bandeja de habilidades propuestas (pantallas) | 36 | `feat/skill-proposals-ui` |

### Ola 4: diseños (solo documentos; pueden ir en paralelo con todo)

| Prompt | Qué hace | Depende de | Rama |
|---|---|---|---|
| [40](40-diseno-kanban.md) | Diseño: tablero Kanban de los bots | ninguna (puede ir en paralelo con todo) | `docs/diseno-kanban` |
| [41](41-diseno-cabina-web.md) | Diseño: cabina web para usar tus bots en el navegador | ninguna (puede ir en paralelo con todo) | `docs/diseno-cabina-web` |
| [42](42-diseno-graficas.md) | Diseño: gráficas y visualizaciones en el chat y las mini-apps | ninguna (puede ir en paralelo con todo) | `docs/diseno-graficas` |
| [43](43-diseno-servicios.md) | Diseño: tableros de servicios (GitHub primero) | ninguna (puede ir en paralelo con todo) | `docs/diseno-servicios` |
| [44](44-diseno-dictado.md) | Diseño: estudio de dictado global | ninguna (puede ir en paralelo con todo) | `docs/diseno-dictado` |
| [45](45-diseno-rutinas-aprobacion.md) | Diseño: paso de aprobación humana en las rutinas por nodos | ninguna (puede ir en paralelo con todo) | `docs/diseno-rutinas-aprobacion` |
| [46](46-diseno-reuniones.md) | Diseño: notas de reuniones sin bot | ninguna (puede ir en paralelo con todo) | `docs/diseno-reuniones` |

### Ola 5: cierre

| Prompt | Qué hace | Depende de | Rama |
|---|---|---|---|
| [99](99-cierre-integracion.md) | Cierre: integrar todo y verificar | todos los anteriores que quieras incluir | `integrate/final` |

## Cómo usarlos
1. Corre el **00** primero y espera a que `integrate/upstream-sync` esté subida a `origin`.
2. Después puedes lanzar en paralelo los de la ola 1 (cada uno en su propio worktree; no se pisan). Con 8 GB de RAM, **no corras más de 2 o 3 agentes que compilen a la vez**: elige los que tocan capas distintas (por ejemplo uno de host y otro de escritorio).
3. Los de la ola 2 y 3 se lanzan cuando termine su dependencia (columna «Depende de»).
4. Los diseños (ola 4) casi no usan máquina: pueden correr siempre.
5. Al final, el **99** integra y verifica.

## Sugerencia para empezar
1. **00** (base). 2. En paralelo **03** (arreglos de los check-ins y del WebView) y **11** (comandos prohibidos). 3. **10**, **12** y **14**. 4. **16** (probar Slack), luego **17**.

## Qué no cubren estos prompts
- Probar Slack con un workspace real (lo haces tú con la lista de comprobación del prompt 16).
- Compilar iOS (requiere un Mac).
- Subir la versión o crear etiquetas: se decide después de validar.
