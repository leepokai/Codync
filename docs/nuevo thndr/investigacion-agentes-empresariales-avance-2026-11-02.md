# Investigación de agentes y herramientas para Codync — avance

Fecha: 2026-11-02. Estado: **incompleto**. No representa la investigación exhaustiva solicitada: faltan la mayoría de repositorios, la lectura íntegra de los 22 documentos existentes y la inspección de código fuente concreta necesaria para afirmar qué puede reutilizarse directamente. No se clonaron repositorios. Los puntos siguientes son propuestas de Codync, no una afirmación de que cada proyecto implemente cada punto.

## Base arquitectónica comprobada

La [arquitectura de Codync](../architecture/overview.md) asigna bots, procesos ACP, sesiones, rutinas, memoria, conectores MCP y validación al host Rust; SQLite guarda estado local. Los clientes presentan estado e intenciones del host. La pantalla remota usa WebRTC, y las conexiones remotas se cifran extremo a extremo, aunque metadatos de encaminamiento siguen siendo visibles. Toda propuesta de UI debe abarcar los clientes aplicables.

La carpeta contiene 22 documentos existentes. Un índice inicial identifica ideas previas sobre bloqueo de bucles, límites de herramientas, aprobaciones, MCP, memoria, rutinas, colaboración y observabilidad; es necesario comparar cada propuesta con su implementación actual y con todos esos documentos antes de tratarla como nueva. La existencia de una propuesta en un documento no demuestra que ya esté implementada.

## Browser Use

[Repositorio y README](https://github.com/browser-use/browser-use). La página describe biblioteca Python, CLI y servicio alojado; identifica licencia MIT en el repositorio. La integración de terceros y las dependencias requieren revisión separada. Recomendación preliminar: experimentar con proceso opcional local o herramienta compatible con el agente, sin convertir Python en dependencia obligatoria del host. Las siguientes son hipótesis de producto derivadas de sus modos de uso, pendientes de validar en código:

1. Sesión de navegador aislada por tarea, con ciclo de vida gestionado por host.
2. Modo opcional con perfil temporal y borrado controlado al finalizar.
3. Concesión explícita de acceso a un perfil existente y sus credenciales.
4. Lista de dominios autorizados para una rutina.
5. Inspección de elementos como complemento del control visual existente.
6. Confirmación antes de enviar formularios, pagar o eliminar.
7. Captura del estado observable antes de una acción sensible.
8. Registro de navegación con redacción de información confidencial.
9. Adjuntos descargados en un directorio autorizado por tarea.
10. Reanudación humana ante autenticación o CAPTCHA, sin intentar evadirlos.
11. Control explícito de pestañas y cierre de recursos al cancelar.
12. Estado de ejecución y fallo accesible desde todos los clientes.
13. Presupuesto de pasos y tiempo para prevenir bucles.
14. Pruebas con un sitio local controlado y datos ficticios.
15. Comparación de latencia y mantenimiento entre CLI, biblioteca y servicio alojado.

## Rerun

[README y arquitectura declarada](https://github.com/rerun-build/rerun): compañeros con memoria, tareas programadas, navegador observable, supervisión humana y catálogo de integraciones; el repositorio anuncia AGPL-3.0. La edición autoalojada y la nube no incluyen necesariamente las mismas prestaciones. No se ha estudiado la compatibilidad de licencia con Codync ni existe autorización legal para copiar código.

1. Plantillas de compañero con función y límites explícitos.
2. Asistente de configuración que transforme un objetivo en una propuesta revisable.
3. Vista unificada de tareas activas y responsables.
4. Encadenamiento explícito de tareas entre bots existentes.
5. Aprobación humana para acciones con efectos externos.
6. Mostrar qué acción concreta espera aprobación y sus argumentos.
7. Delegar una aprobación a una persona autorizada.
8. Mostrar estado del navegador y permitir toma de control humana.
9. Retomar una tarea después de la intervención humana.
10. Plantillas de rutinas con previsualización antes de activarlas.
11. Conectores visibles con estado y permisos concedidos.
12. Memoria por bot con controles de consulta y eliminación.
13. Resumen periódico de tareas con enlaces a sus evidencias.
14. Agrupación de resultados de varios bots en un único informe.
15. Diagnóstico de la máquina que ejecuta cada tarea.

Estas son ideas de adaptación; varias coinciden con capacidades existentes o propuestas previas y necesitan contraste de código. El host mantendría la lógica y los clientes representarían los mismos estados.

## ascii-art

[Repositorio](https://github.com/0xSero/ascii-art): convierte imágenes en arte de puntos y animaciones; GitHub indica MIT. Es una referencia visual, no un sistema de agentes. La reutilización de scripts, imágenes y dependencias requiere revisar archivos y atribuciones individuales.

1. Prototipo de avatar de bot a partir de puntos.
2. Variante estática para accesibilidad y movimiento reducido.
3. Animación suave del estado activo con límite de consumo.
4. Imagen estática para widgets y terminales compatibles.
5. Generación anticipada para no procesar imágenes en cada renderizado.
6. Paletas con contraste verificable en modo claro y oscuro.
7. Siluetas reconocibles a tamaños pequeños.
8. Representación alternativa en terminales sin gráficos.
9. Parámetros de densidad adaptados a resolución y tamaño.
10. Vista previa antes de aplicar un avatar.
11. Opción para desactivar animaciones por dispositivo.
12. Evitar transmitir imágenes privadas a servicios externos sin consentimiento.
13. Guardar la imagen derivada con límites de tamaño.
14. Prueba de legibilidad en iOS, Android y escritorio.
15. Comparar su estilo con el plan de avatares existente antes de adoptarlo.

## Comprobaciones y advertencias

- [n8n describe su licencia como fair-code](https://github.com/n8n-io/n8n-docs/blob/main/docs/n8n-community-license/README.md), con restricciones comerciales; no asumir que todo repositorio público es código abierto aprobado por OSI.
- [MCP Servers](https://github.com/modelcontextprotocol/servers) es un repositorio de implementaciones de referencia; verificar el estado y ubicación actual de cada servidor antes de prometer conectores concretos.
- [Swarm](https://github.com/openai/swarm) es experimental y educativo; su sucesor indicado es Agents SDK. No proponerlo como dependencia de producción sin evaluación actualizada.
- El README de [E2B](https://github.com/e2b-dev/E2B) identifica Apache-2.0 para el repositorio, pero eso no garantiza operación local, gratuidad ni aislamiento perfecto para una configuración concreta.
- Las afirmaciones de latencia, compatibilidad, licencia de componentes y soporte empresarial requieren mediciones y fuentes específicas, no deducciones a partir de una portada.

## Pendiente imprescindible

Leer íntegramente los documentos locales; revisar cada uno de los proyectos enumerados y sus licencias, documentación, ejemplos y componentes concretos; producir 15–20 hallazgos útiles por repositorio con referencias precisas y distinción entre funcionalidad verificada, propuesta e integración viable; contrastar cada hallazgo con código de Codync; priorizar y especificar costes, pruebas y riesgos. También revisar la separación AutoGen/AG2 y confirmar todas las URLs y licencias actuales.

## Inventario de todos los repositorios enviados: primera lectura

Las 50 páginas de repositorio fueron abiertas el 2026-11-02, sin clonación. Esta sección es un **inventario preliminar basado en las páginas públicas**, no una auditoría de sus árboles de código ni un análisis exhaustivo de 15–20 hallazgos por proyecto. Las propuestas son inferencias para Codync, no funciones necesariamente existentes en los proyectos. Para reutilizar código o distribuir dependencias se debe revisar la licencia exacta del componente y sus dependencias, las versiones y sus obligaciones. Prioridad: A = investigar pronto; B = después de un caso de uso concreto; C = referencia o especialización.

### Estado de Codync que cambia la evaluación

- [Computer use](../features/computer-use.md): ya existe un MCP `computer`, control por elementos y píxeles, un bot a la vez, control humano y `type_login` con restricciones. Se excluyen actualmente las herramientas `browser_*` del driver. El historial de actividad existe en host, pero la pantalla correspondiente sigue pendiente según el documento.
- [Políticas MCP](../features/mcp-tool-policies.md): ya existen reglas allow/ask/deny, aprobaciones ligadas a argumentos exactos, revocación y recibos. La política no aísla accesos independientes del harness; una anotación `readOnlyHint` no prueba ausencia de efectos. El documento indica que las pantallas Apple están excluidas por instrucción en esa entrega; su paridad actual requiere comprobarse.
- [Rutinas](../features/routines.md): ya hay horarios, webhook, eventos, observación de archivos y URLs, historial, control de concurrencia y persistencia. No presentar estos disparadores como si fueran nuevos.
- [Colaboración](../features/bot-collaboration.md): ya hay `ask_bot`, `message_bot`, administración acotada de equipo, límites, detección de ciclos y estados de interrupción. Una propuesta de grafo debe justificar qué añade a esto.
- [Memoria](../features/context-and-memory.md), [voz](../features/voice-call.md) e [historial de ejecución](../features/search-and-run-history.md) ya tienen fundamentos propios. Investigar las lagunas antes de agregar servicios paralelos.

En cada ficha, **Ideas** son hipótesis de diseño; **Integración** es una vía candidata, no código probado.

### 1. [Browser Use](https://github.com/browser-use/browser-use) — A

Biblioteca y CLI para agentes de navegador, con oferta cloud separada. Ideas: navegación con DOM además de visión; perfiles efímeros; descargas por tarea; evidencia de confirmaciones. Integración: evaluar herramienta externa opcional controlada por host y comparar con el driver actual; no asumir que las políticas MCP gobiernan todo el proceso. Hay 15 propuestas en la ficha anterior.

### 2. [Open Interpreter](https://github.com/openinterpreter/open-interpreter) — B

La URL redirige a `openinterpreter/openinterpreter`; su presentación actual destaca agentes de programación y modelos abiertos. Ideas: inspeccionar interacción local; permisos por acción; salida de archivos como artefactos; interrupción y recuperación. Integración: evaluar compatibilidad con ACP o proceso opcional; no ejecutar código generado en el host sin aislamiento.

### 3. [UI-TARS Desktop](https://github.com/bytedance/UI-TARS-desktop) — A

Pila de agente multimodal para GUI. Ideas: comparar capturas y árboles de accesibilidad; diagnosticar objetivo visual ambiguo; pruebas con cambios de resolución; control humano durante ejecución. Integración: comparar como proveedor opcional con `cua-driver`, sin reemplazar los límites y permisos de Codync.

### 4. [Crawl4AI](https://github.com/unclecode/crawl4ai) — A

Rastreo y extracción de contenido web para modelos. Ideas: extracción por URL como herramienta; conservar origen y fecha; límites de profundidad y tamaño; limpiar navegación repetida. Integración: proceso local opcional o servicio elegido, con controles SSRF y sin introducir una segunda fuente de verdad para memoria.

### 5. [n8n](https://github.com/n8n-io/n8n) — B

Automatización visual; licencia fair-code, no asumir licencia abierta sin restricciones. Ideas: importar eventos de flujos; disparar una rutina por webhook; vincular una ejecución externa con su resultado; mostrar credenciales necesarias. Integración: conexión por webhook/API sujeto a autenticación y licencia; evitar incrustar componentes sin revisión.

### 6. [Activepieces](https://github.com/activepieces/activepieces) — B

Automatización mediante piezas; el README distingue edición comunitaria MIT de funciones comerciales. Ideas: catálogo de conectores; disparadores y acciones con esquema; diagnóstico de credenciales; seguimiento por ejecución. Integración: evaluar API/MCP o piezas comunitarias concretas y su licencia, no asumir que todas las funciones enterprise son MIT.

### 7. [Sim](https://github.com/simstudioai/sim) — B

Espacio colaborativo para flujos de agentes. Ideas: configuración visible de entradas; pausas para revisión; historial por paso; versiones de configuración. Integración: conectar como servicio externo solo si aporta un flujo necesario; mantener la decisión y eventos de Codync en el host.

### 8. [Dify](https://github.com/langgenius/dify) — B

Plataforma para aplicaciones, flujos y RAG. Ideas: documentos con fuentes citables; probar un flujo con entradas de ejemplo; separar configuración de producción; supervisar errores de recuperación. Integración: evaluar API externa o inspiración para contratos; comprobar licencia específica y condiciones de marca.

### 9. [Langflow](https://github.com/langflow-ai/langflow) — C

Constructor visual de agentes y flujos. Ideas: editor de conexiones legible; validación de entradas por nodo; pruebas de pasos individuales; exportación de configuraciones. Integración: referencia de diseño; evitar crear un editor visual nuevo antes de identificar demanda.

### 10. [Flowise](https://github.com/FlowiseAI/Flowise) — C

Herramienta visual para agentes. Ideas: catálogo de bloques; previsualizar herramientas disponibles; representar dependencias de datos; ejecutar casos de prueba guardados. Integración: eventual servicio externo con credenciales del usuario; revisar componentes y licencia antes de incorporar código.

### 11. [CrewAI](https://github.com/crewAIInc/crewAI) — B

Orquestación de agentes por roles. Ideas: plantillas de responsabilidades; entregables esperados; límites de delegación; resumen de aportaciones por bot. Integración: extender contratos de equipo del host, que ya dispone de mensajería y administración de bots; no duplicar su runtime.

### 12. [LangGraph](https://github.com/langchain-ai/langgraph) — A

Motor de agentes y flujos con estado. Ideas: puntos de control explícitos; representar interrupciones; versionar estado de un flujo; reanudar solo etapas seguras. Integración: estudiar patrones de durabilidad y migraciones para el host Rust; no suponer que reanudar una acción externa sea seguro.

### 13. [Microsoft AutoGen](https://github.com/microsoft/autogen) — B

Framework para aplicaciones agénticas. Ideas: mensajes entre especialistas con contratos; registrar procedencia de respuestas; limitar conversaciones cíclicas; pruebas de equipos con agentes simulados. Integración: contrastar con los límites existentes de `ask_bot`. AG2 es un proyecto distinto; no se proporcionó su URL y no debe tratarse como el mismo repositorio.

### 14. [MetaGPT](https://github.com/geekan/MetaGPT) — B

La URL redirige a `FoundationAgents/MetaGPT`; propone trabajo multiagente por roles. Ideas: procedimientos operativos configurables; revisión de entregables; responsabilidades por etapa; trazabilidad de decisiones. Integración: plantillas sobre bots y rutinas existentes; validar que las funciones sean útiles fuera de ingeniería de software.

### 15. [Agno](https://github.com/agno-agi/agno) — B

Plataforma y framework de agentes. Ideas: contratos para agentes y equipos; sesiones con contexto acotado; capacidades optativas; diagnóstico de runtime. Integración: proceso o servicio externo bajo demanda; medir consumo antes de prometer mejoras de velocidad.

### 16. [Pydantic AI](https://github.com/pydantic/pydantic-ai) — A

Framework Python con interfaces y datos tipados. Ideas: validar esquemas de entrada; rechazar salidas inválidas con error explicable; tipos explícitos para resultados; pruebas con modelos simulados. Integración: adoptar el patrón en Rust/Serde y ACP sin introducir Python obligatorio; la validación no garantiza verdad semántica.

### 17. [Onyx](https://github.com/onyx-dot-app/onyx) — A

Plataforma de IA y búsqueda sobre fuentes. Ideas: conectores con sincronización incremental; mostrar procedencia; respetar permisos de origen; quitar resultados cuando se revoca acceso. Integración: índice externo opcional o consultas federadas; comprobar ACL y revocación antes de usar datos empresariales.

### 18. [Chatwoot](https://github.com/chatwoot/chatwoot) — B

Bandeja de soporte multicanal. Ideas: vincular conversación de soporte a un bot; borrador antes de responder; derivación a humano; marcar estado y asignación. Integración: webhook/API bajo autorización explícita; deduplicar eventos y no enviar mensajes sin permiso.

### 19. [RAGFlow](https://github.com/infiniflow/ragflow) — A

Motor de recuperación y procesamiento documental. Ideas: citas por página; extracción de tablas; resultados con calidad y fuente; volver a indexar una versión cambiada. Integración: servicio optativo para documentos complejos, con retención y permisos propios visibles.

### 20. [Mem0](https://github.com/mem0ai/mem0) — B

Capa de memoria para agentes. Ideas: procedencia de recuerdos; detección de contradicciones; caducidad y olvido; selección de contexto pertinente. Integración: comparar evaluación con `codync-mem` existente; no sustituir memoria ni enviar datos a terceros por defecto.

### 21. [Khoj](https://github.com/khoj-ai/khoj) — B

Asistente autoalojable con búsqueda y automatizaciones. Ideas: buscar notas y documentos; responder con referencias; programar resúmenes; filtrar fuentes por autorización. Integración: conectar una instancia del usuario o estudiar UX de búsqueda; verificar disponibilidad de cada fuente y costes.

### 22. [MindsDB](https://github.com/mindsdb/mindsdb) — B

Plataforma para conectar datos y capacidades de IA. Ideas: catálogo de fuentes SQL; consultas con presupuesto; explicación y previsualización; guardar procedencia y resultado. Integración: conector opcional con credenciales de solo lectura y límites de red, tiempo y filas.

### 23. [CopilotKit](https://github.com/CopilotKit/CopilotKit) — C

Componentes y herramientas para integrar agentes en aplicaciones. Ideas: aprobar acciones desde contexto; presentar estado de ejecución; asociar adjuntos a una operación; UI de errores recuperables. Integración: inspiración de interfaz en todos los clientes; incrustar React solo donde realmente corresponda.

### 24. [Semantic Kernel](https://github.com/microsoft/semantic-kernel) — B

SDK de Microsoft para integrar capacidades de IA y plugins. Ideas: contratos de funciones; separación de proveedor; filtros antes de ejecutar herramientas; pruebas de plugins simulados. Integración: estudiar interoperabilidad y MCP en vez de reemplazar la arquitectura ACP del host.

### 25. [LiveKit Agents](https://github.com/livekit/agents) — B

Framework de agentes de voz sobre infraestructura LiveKit. Ideas: interrupción de voz; métricas de latencia por etapa; reconexión de medios; transferencia a humano. Integración: comparar con la [voz existente](../features/voice-call.md); WebRTC no garantiza una latencia concreta ni elimina costes de modelos y telefonía.

### 26. [Pipecat](https://github.com/pipecat-ai/pipecat) — B

Framework para flujos de voz y vídeo. Ideas: canalización STT/modelo/TTS intercambiable; interrupciones; detección de silencios; medición extremo a extremo. Integración: adaptador o servicio opcional frente a implementación propia; estudiar privacidad y coste de cada proveedor.

### 27. [MCP Servers](https://github.com/modelcontextprotocol/servers) — A

Implementaciones de referencia y recursos de MCP. Ideas: comprobar salud de servidor; muestras de instalación reproducible; definir permisos por herramienta; pruebas de descubrimiento y revocación. Integración: catálogo de servidores concretos con versiones y licencias comprobadas; no asumir que todos los conectores de la lista original siguen incluidos.

### 28. [E2B](https://github.com/e2b-dev/E2B) — A

Infraestructura y SDK para entornos de ejecución. Ideas: aislar código generado; fijar tiempo y memoria; exportar resultados de forma explícita; destruir entornos caducados. Integración: proveedor opcional frente a alternativas locales; revisar despliegue, red, confianza y costes antes de elegir.

### 29. [Langfuse](https://github.com/langfuse/langfuse) — A

Observabilidad y evaluación de aplicaciones de IA. Ideas: correlación de ejecución, turno y herramienta; contabilizar latencia; presupuestos y alertas; redactar datos sensibles. Integración: exportador optativo y consentimiento explícito; evaluar qué telemetría recibe de ACP antes de prometer trazas completas.

### 30. [Phoenix](https://github.com/Arize-ai/phoenix) — A

Herramientas para trazas y evaluación de IA. Ideas: evaluar recuperación; comparar versiones de prompts; examinar errores de herramientas; registrar muestras anonimizadas. Integración: exportación y evaluación externa opcionales; no registrar razonamiento privado ni datos sin autorización.

### 31. [Vanna](https://github.com/vanna-ai/vanna) — B

Agente orientado a consultas de datos. Ideas: generar SQL para revisión; mostrar tablas y filtros utilizados; limitar filas y duración; guardar ejemplos de consultas aceptadas. Integración: herramienta externa con usuario de base de datos restringido; validar SQL y usar transacciones de solo lectura aplicadas por la base.

### 32. [OpenBB](https://github.com/OpenBB-finance/OpenBB) — C

La URL redirige a `openbq-org/OpenBB`; plataforma orientada a datos financieros. Ideas: conectores con fecha de datos; trazabilidad de proveedor; unidades explícitas; gráficos reproducibles. Integración: fuente especializada optativa; revisar licencias y términos de cada proveedor de datos.

### 33. [LocalAI](https://github.com/mudler/LocalAI) — B

Motor autoalojable de modelos. Ideas: configurar endpoint local; comprobar capacidades del modelo; pruebas sin acceso externo; informar uso de CPU y memoria. Integración: mediante proveedor compatible con el harness, cuando lo soporte; el carácter local del servidor no demuestra que todos sus modelos y dependencias funcionen desconectados.

### 34. [smolagents](https://github.com/huggingface/smolagents) — B

Biblioteca de agentes que usan código como acción. Ideas: conservar código generado para revisión; limitar importaciones; capturar salidas como archivos; reproducir una ejecución con datos de prueba. Integración: estudio de sandbox y herramienta opcional; ejecutar Python arbitrario no es seguro por sí mismo.

### 35. [TaskWeaver](https://github.com/microsoft/TaskWeaver) — B

Framework code-first para análisis de datos. Ideas: resultados tabulares como objetos; separar planificación de ejecución; registrar versiones de datos; exportar resultados reproducibles. Integración: proceso aislado optativo; medir coste y compatibilidad frente a herramientas existentes.

### 36. [AppAgent](https://github.com/mnotgod96/AppAgent) — C

La URL redirige a `TencentQQGYLab/AppAgent`, investigación de agentes para apps móviles. Ideas: pruebas de interacción en emulador; capturas antes/después; biblioteca de tareas de prueba; evaluación de reconocimiento visual. Integración: prototipo de laboratorio con dispositivos autorizados; no confundir controlar un teléfono con el control remoto de escritorio existente.

### 37. [MobileAgent](https://github.com/X-PLUG/MobileAgent) — C

Familia de agentes GUI. Ideas: evaluación de acciones táctiles; recuperación ante cambios de pantalla; registro de errores de localización; escenarios de prueba multiplataforma. Integración: investigación experimental de modelos y licencias por versión; no prometer soporte móvil de producción.

### 38. [LlamaIndex](https://github.com/run-llama/llama_index) — B

Plataforma de procesamiento documental y componentes de agentes. Ideas: ingestión incremental; segmentación por documento; citas verificables; evaluación de recuperación. Integración: servicio de indexación opcional, con ACL y borrado propagados; diferenciar componentes locales y servicios comerciales.

### 39. [Haystack](https://github.com/deepset-ai/haystack) — B

Framework modular de pipelines de IA y recuperación. Ideas: etapas de ingestión separadas; validación de entrada y salida; evaluar ranking; sustituir componentes sin cambiar el contrato. Integración: referencia para un contrato de recuperación del host o servicio optativo; comprobar telemetría y configuración.

### 40. [DSPy](https://github.com/stanfordnlp/dspy) — C

Framework para programar y optimizar sistemas basados en modelos. Ideas: colección de ejemplos; métricas de evaluación; comparación entre versiones; evitar ajustar con datos de producción sin permiso. Integración: herramienta de investigación fuera del runtime; la optimización no garantiza corrección ni elimina la necesidad de evaluar.

### 41. [Instructor](https://github.com/jxnl/instructor) — B

La URL redirige a `567-labs/instructor`; extracción de salidas estructuradas. Ideas: esquemas explícitos; errores de validación entendibles; reintentos acotados; pruebas con entradas incompletas. Integración: reproducir el contrato en Rust o usar la biblioteca en un adaptador opcional; un esquema válido puede contener información incorrecta.

### 42. [AnythingLLM](https://github.com/Mintplex-Labs/anything-llm) — B

Aplicación de agentes local-first y documentos. Ideas: agrupar documentos por espacio; selección explícita de fuentes; estado de indexación; borrado de contenidos. Integración: comparar gestión de espacios con el ámbito de bots y carpetas existente; validar seguridad y licencias por componente.

### 43. [LibreChat](https://github.com/danny-avila/LibreChat) — B

La URL redirige a `LibreChat-AI/LibreChat`; interfaz de chat para varios proveedores y herramientas. Ideas: selección de modelo; administrar conexiones MCP; búsqueda de mensajes; artefactos asociados a conversación. Integración: evaluar patrones de UX y adaptadores concretos; mantener ejecución y autorización en el host.

### 44. [Promptfoo](https://github.com/promptfoo/promptfoo) — A

Evaluación y pruebas de seguridad para aplicaciones de IA. Ideas: fixtures contra inyección; probar rechazo de llamadas indebidas; comparar resultados por versión; ejecutar pruebas en CI. Integración: herramienta de desarrollo externa con datos sintéticos; no ejecutar pruebas contra sistemas ajenos sin autorización.

### 45. [AgentOps](https://github.com/AgentOps-AI/agentops) — B

SDK de seguimiento para agentes y modelos. Ideas: coste por ejecución cuando haya datos; duración y errores; detectar repeticiones; correlacionar eventos. Integración: exportación opcional y redactada; los costes desconocidos deben mostrarse como desconocidos.

### 46. [OpenHands](https://github.com/All-Hands-AI/OpenHands) — B

La URL redirige a `OpenHands/OpenHands`; entorno de desarrollo autónomo. Ideas: aislar espacio de trabajo; visualizar cambios; capturar pruebas ejecutadas; reanudar con estado explícito. Integración: estudiar compatibilidad ACP y sandbox como opciones; Codync ya ejecuta agentes de programación.

### 47. [SWE-agent](https://github.com/princeton-nlp/SWE-agent) — B

La URL redirige a `SWE-agent/SWE-agent`; agente para tareas de repositorio. Ideas: convertir un issue en tarea trazable; registrar comandos y resultados; verificar con pruebas; comparar cambios antes de aplicarlos. Integración: fixture de evaluación o agente opcional, sin suponer compatibilidad ACP sin comprobarla.

### 48. [Swarm](https://github.com/openai/swarm) — C

Ejemplo educativo de coordinación; el proyecto apunta al Agents SDK como evolución. Ideas: contratos de traspaso; alcance de contexto por agente; límite de transferencias; pruebas de ciclos. Integración: estudiar conceptos, no adoptar el repositorio experimental como runtime de producción.

### 49. [ascii-art](https://github.com/0xSero/ascii-art) — C

Arte de puntos y animación a partir de imágenes. Ideas: avatares generados; versión estática; contraste; compatibilidad con movimiento reducido. Integración: experimento visual, con revisión de licencias por recurso y adaptación a cada cliente. Hay 15 propuestas en la ficha anterior.

### 50. [Rerun](https://github.com/rerun-build/rerun) — A

Plataforma autoalojable de compañeros de IA; el repositorio anuncia AGPL-3.0. Ideas: plantillas de roles; tablero de trabajo; toma de control humano; entregables entre bots. Integración: inspiración de producto sobre los bots, rutinas y pantalla existentes; revisar implicaciones AGPL antes de reutilizar código. Hay 15 propuestas en la ficha anterior.

## Próxima fase para cumplir el alcance original

Este inventario cubre las 50 URLs, pero todavía faltan: leer los README y documentación relevantes completos y seleccionar archivos concretos; revisar licencias y versiones de los componentes candidatos; examinar íntegramente los documentos locales, incluidos los de `prompts/`; y desarrollar entre 15 y 20 hallazgos específicos por cada repositorio, con referencias profundas, comparación con el código y plan de validación. Abrir una portada no demuestra que se haya auditado un repositorio entero. No se ha instalado ni ejecutado ninguno de estos proyectos.

# Evaluación práctica e implementación por repositorio

Fecha: 2026-11-02. Esta ampliación da una recomendación aplicable para cada una de las 50 URLs. **Alcance de evidencia:** se consultaron las páginas públicas de los proyectos y fuentes específicas enlazadas en algunos casos; no se hizo auditoría completa de código, no se instalaron los proyectos ni se verificó compatibilidad de APIs. Las propuestas y rutas de implementación son diseño preliminar, no hallazgos probados ni promesas de compatibilidad. Las fichas anteriores enlazan cada repositorio. Los 15–20 hallazgos rigurosos por repositorio solicitados al inicio siguen pendientes: no se rellenan con ideas repetidas para aparentar exhaustividad.

## Contrato común antes de integrar

1. **Elegir el nivel:** inspiración implementada en Rust; proceso local opcional; conexión a servicio del usuario; o herramienta de desarrollo. No integrar una plataforma completa para obtener una sola función. Código copiado requiere revisar licencia, ficheros, dependencias y avisos; la licencia del repositorio no basta para cada modelo o servicio.
2. **Host dueño:** la lógica, credenciales, procesos, persistencia, cuotas, autorización y eventos deben vivir en `host/`; las aplicaciones y TUI reflejan estado por los contratos existentes. No confundir implementación propuesta con una API ya existente: los nombres de eventos o tipos descritos abajo son sugerencias de diseño.
3. **Permisos:** el proxy de políticas MCP existente controla llamadas que pasan por él, no un proceso ACP arbitrario. Un adaptador propio debe definir su propio límite de ejecución y pasar por comprobaciones de autorización pertinentes. El aislamiento de sistema operativo, red, archivos y credenciales debe verificarse por separado.
4. **Recuperación:** antes de reintentar un efecto externo, comprobar si su resultado es conocido. Un timeout puede significar que la operación sí ocurrió. Añadir identificación de operación, recibo y estado `desconocido`; nunca repetir automáticamente un envío o pago incierto.
5. **Validación:** fixtures locales para red y errores, pruebas de autorización y cancelación, recuperación tras reinicio y comprobaciones UI de iOS, Android, escritorio y TUI cuando corresponda. Para procesos nativos comprobar macOS, Linux y Windows; declarar limitaciones concretas. Las funciones opcionales deben fallar de forma visible si falta su dependencia.
6. **Información sensible:** por defecto no exportar transcripciones, capturas, credenciales, datos de clientes ni razonamiento del modelo; habilitar destinos y retención mediante decisión explícita. Las fuentes recuperadas son datos no confiables y no adquieren autoridad de instrucciones.

En cada ficha: **valor** es recomendación; **agregar** describe el cambio propuesto; **hacerlo** señala su integración candidata; **comprobar** define una prueba observable. La selección es deliberadamente más corta que los 15–20 puntos originales.

## Navegador y control de interfaces

### 1. Browser Use — valor alto, adaptador opcional

**Agregar:** navegación estructurada de páginas, sesiones aisladas por tarea y descargas asociadas al resultado; vista de estado y relevo humano ante autenticación.

**Hacerlo:** prototipo de proceso local bajo supervisión del host que exponga operaciones de navegador tipadas mediante una interfaz controlada; conservar el MCP `computer` actual para aplicaciones generales. Definir una carpeta temporal por ejecución, permitir solo exportaciones elegidas y separar los perfiles nuevos de los del usuario. Antes de seleccionar CLI, biblioteca o MCP, verificar el mecanismo de integración en la versión elegida. El [README](https://github.com/browser-use/browser-use) distingue biblioteca, CLI y servicio alojado.

**Comprobar:** sitio ficticio con formularios y descarga; detener durante una navegación; pedir aprobación antes de envío; verificar que otro bot no acceda al perfil y que los archivos temporales se limpien.

### 2. Open Interpreter — valor condicionado, inspiración

**Agregar:** una vista de código y resultado de acciones generadas, con aprobación antes de ejecutar y archivos entregables visibles.

**Hacerlo:** conservar ACP como runtime; modelar en host un resultado de ejecución que enlace comando aprobado, estado, salida limitada y archivos. Considerar Open Interpreter como proceso separado solo tras validar integración, no como reemplazo del host.

**Comprobar:** comando rechazado no se ejecuta; salida larga se limita; cancelación detiene el proceso y no muestra secretos.

### 3. UI-TARS Desktop — valor alto para evaluación GUI

**Agregar:** pruebas comparativas para localizar controles por captura y por árbol de accesibilidad, y una explicación cuando el objetivo visual sea ambiguo.

**Hacerlo:** crear una suite externa de pantallas de prueba; comparar resultados contra el driver de `host/src/screen/cua.rs`. Solo si supera las pruebas, valorar proveedor visual opcional sin saltarse las reglas de toma de control.

**Comprobar:** escalado, idioma, ventana tapada, objetivo duplicado y control que desaparece; una acción ambigua debe detenerse.

### 4. Crawl4AI — valor alto para lectura web

**Agregar:** herramienta de extracción que devuelva texto limpio, URL final, fecha y avisos de extracción, con límites de tamaño.

**Hacerlo:** evaluar servicio o proceso aislado bajo demanda; el host valida URL, redirecciones y límites antes de despachar, y entrega al agente datos marcados como no confiables. Examinar `host/src/research.rs` antes de crear funciones equivalentes.

**Comprobar:** bloqueo de red privada y redirección a IP interna; página excesiva; HTML con instrucciones maliciosas; citas que conservan URL.

## Automatización visual

### 5. n8n — valor alto si la empresa ya lo usa; conexión externa

**Agregar:** disparar una rutina de Codync desde un flujo y consultar su resultado; permitir que el usuario configure el flujo en su instalación n8n.

**Hacerlo:** utilizar webhooks ya previstos en `host/src/routines/` y un adaptador de recepción con firma o secreto, deduplicación y expiración; documentar qué extremo recibe cada dato. No incorporar código sin analizar la [licencia fair-code](https://github.com/n8n-io/n8n-docs/blob/main/docs/n8n-community-license/README.md).

**Comprobar:** entrega duplicada, firma incorrecta, reintento tras caída y revocación del secreto; no ejecutar dos veces el mismo evento.

### 6. Activepieces — valor medio, catálogo externo

**Agregar:** plantilla para recibir eventos y ejecutar tareas con conectores disponibles en la instalación del usuario.

**Hacerlo:** validar contratos de webhook/MCP por versión, mapear credencial y ámbito a un bot, mostrar el resultado remoto sin mantener estado paralelo. La edición comunitaria y las funciones comerciales deben revisarse por separado.

**Comprobar:** credencial revocada, evento duplicado, conector indisponible y resultado sin datos sensibles.

### 7. Sim — valor medio, inspiración de ejecución

**Agregar:** historial de una automatización con entradas, pasos terminados, espera humana y error visible.

**Hacerlo:** ampliar el inspector de ejecuciones existente en host con eventos estructurados opcionales; representar la misma secuencia en clientes sin trasladar la lógica al canvas.

**Comprobar:** actualizar desde eventos, recuperar tras reconexión y visualizar un paso rechazado sin ejecutar los siguientes.

### 8. Dify — valor medio para conocimiento conectado

**Agregar:** conexión opcional a una aplicación o fuente documental de una instalación controlada por el usuario; referencias a los documentos utilizados.

**Hacerlo:** adapter HTTP del host con secretos almacenados por los mecanismos existentes; fijar espacio autorizado y asociar las citas a la respuesta, sin copiar datos a otro proveedor por defecto. Validar API y licencia de la edición elegida.

**Comprobar:** revocar acceso, respuesta sin citas, timeout y documento retirado; el error no debe inventar fuentes.

### 9. Langflow — valor bajo a medio, inspiración

**Agregar:** previsualización de dependencias de una rutina antes de habilitarla: herramientas, credenciales y entradas necesarias.

**Hacerlo:** generar el resumen en host a partir de una definición explícita; reutilizar la ficha de rutina en los clientes. No crear un editor de grafos hasta tener casos reales que lo exijan.

**Comprobar:** una credencial ausente impide o advierte según política; configuración antigua sigue legible.

### 10. Flowise — valor bajo a medio, integración externa

**Agregar:** ejecutar un flujo remoto permitido con entradas tipadas y recibir su salida como resultado de herramienta.

**Hacerlo:** adaptador opcional con URL permitida, credencial y esquema fijado por propietario; autorización por operación y límites de respuesta. La viabilidad exacta requiere comprobar API y versión.

**Comprobar:** entrada inválida se rechaza antes de red; respuesta excesiva se limita; la caída deja estado incierto si existió efecto.

## Equipos y contratos

### 11. CrewAI — valor medio, inspiración de plantillas

**Agregar:** plantillas de equipos con objetivo, responsabilidades, entregable y límites de cada bot; vista previa antes de crearlos.

**Hacerlo:** ampliar las configuraciones gestionadas por `manage` y reutilizar `ask_bot`/`message_bot`; asignar carpetas y capacidades explícitas, sin agregar CrewAI como segundo coordinador.

**Comprobar:** el bot creado no supera permisos del gestor; una plantilla inválida no crea medio equipo; se respeta el límite existente.

### 12. LangGraph — valor alto, checkpoints duraderos

**Agregar:** ejecución por etapas con punto de control y estado explícito de pausa, fracaso o efecto desconocido para rutinas complejas.

**Hacerlo:** diseñar registro de ejecución versionado en SQLite, con ID de etapa y recibos de acciones externas; reanudar solo etapas deterministas o identificadas como seguras. La [documentación de LangGraph](https://docs.langchain.com/oss/javascript/langgraph/thinking-in-langgraph) explica checkpoints en límites de nodo y repetición del nodo interrumpido: una reanudación no evita efectos duplicados por sí sola.

**Comprobar:** reiniciar antes y después de un efecto; el host debe mostrar incertidumbre si no hay recibo y no reenviar automáticamente.

### 13. AutoGen — valor medio, evaluación de colaboración

**Agregar:** pruebas de intercambio entre roles con datos mínimos y límites de transferencia visibles.

**Hacerlo:** usar fixtures ACP de bots simulados y los contratos existentes de `ask_bot`, en lugar de duplicar su runtime. AG2 es una bifurcación distinta y debe investigarse por separado antes de recomendarla.

**Comprobar:** ciclos A→B→A, destinatario ocupado, cancelación del solicitante y reinicio del host.

### 14. MetaGPT — valor medio, procedimientos reutilizables

**Agregar:** plantillas de procedimiento con fases, entregables esperados y responsable de revisión.

**Hacerlo:** almacenar definición versionada en host, instanciar tareas sobre bots existentes y dejar cada revisión humana explícita; comenzar con un flujo corto de análisis y revisión.

**Comprobar:** fase rechazada no avanza; el resumen enlaza los entregables reales; cancelar conserva lo ya producido.

### 15. Agno — valor medio, diagnóstico de herramientas

**Agregar:** ficha de capacidades declaradas por bot, dependencias disponibles y comprobación previa al turno.

**Hacerlo:** consolidar en host el estado observado de agente, conectores y configuración; distinguir capacidad declarada de conexión comprobada. Tomar el enfoque de plataforma como referencia, sin asumir compatibilidad de procesos.

**Comprobar:** MCP sin iniciar, modelo inexistente y credencial caducada; los clientes presentan el mismo estado.

### 16. Pydantic AI — valor alto, contratos tipados

**Agregar:** validación tipada de entradas y resultados de nuevas herramientas, con errores concretos y ejemplos.

**Hacerlo:** definir estructuras Rust/Serde con límites, variantes y versiones; validar antes de red o efectos. Generar esquemas compartidos para consumidores cuando el contrato lo permita.

**Comprobar:** JSON malformado, campo desconocido o excesivo, versión antigua y resultado válido pero semánticamente falso; este último debe seguir requiriendo verificación.

## Datos, memoria y soporte

### 17. Onyx — valor alto si se necesita búsqueda empresarial

**Agregar:** búsqueda en fuentes autorizadas con citas y respeto a permisos actuales de origen.

**Hacerlo:** primero evaluar conector a una instalación existente, con credencial de usuario y un ámbito concreto; si se indexa localmente, almacenar ACL, origen, versión y borrado. Nunca inferir permisos de una URL conocida.

**Comprobar:** retirar permiso tras indexar, borrar documento, cambiar de cuenta y verificar que no aparecen fragmentos antiguos.

### 18. Chatwoot — valor medio para soporte

**Agregar:** recibir tickets asignados, crear borrador y solicitar aprobación para contestar o transferir a humano.

**Hacerlo:** adaptar eventos autenticados a rutinas del host y vincular cada ticket con identificadores de canal y cuenta; enviar mediante herramienta con argumentos exactos y recibo.

**Comprobar:** evento duplicado, cuenta distinta, rechazo humano y pérdida de conexión después de enviar; no volver a enviar ante resultado desconocido.

### 19. RAGFlow — valor alto para documentos complejos

**Agregar:** extracción de PDF con referencias de página, tablas y estado de calidad de extracción.

**Hacerlo:** proceso o servicio opcional en el host; resultado normalizado con documento, versión, página y texto; permisos de acceso heredados del archivo de origen. Revisar API y requisitos operativos antes de incluirlo.

**Comprobar:** PDF escaneado, tabla partida, archivo sustituido y fuente inaccesible; mostrar ausencia de cita cuando no pueda localizarse.

### 20. Mem0 — valor medio, comparar memoria

**Agregar:** procedencia y vigencia de recuerdos, corrección o eliminación por usuario y pruebas de contradicciones.

**Hacerlo:** comparar `host/crates/memory/` y `host/src/chat/memory/` con los requisitos antes de cambiar esquema; extender la memoria propia si falta alguna capacidad, sin crear un almacén paralelo por defecto.

**Comprobar:** borrar un hecho y confirmar que no se recupera; dos fuentes contradictorias deben conservar su procedencia.

### 21. Khoj — valor medio, búsqueda personal

**Agregar:** búsqueda por fuentes seleccionadas y respuestas con enlaces verificables a archivos o notas autorizados.

**Hacerlo:** evaluar conexión externa o un contrato interno de búsqueda; reutilizar `host/src/api/saved_searches.rs` donde corresponda y separar índice de fuente y ámbito del usuario.

**Comprobar:** cuenta cambiada, archivo movido, fuente desconectada y búsqueda sin resultados.

### 22. MindsDB — valor medio solo con demanda de datos

**Agregar:** catálogo de conexiones y herramienta de consultas con vista previa, límite de filas y tiempo.

**Hacerlo:** credencial de solo lectura reforzada en la base, acceso de red limitado y aprobación para cualquier operación con efectos. Evaluar conexión mediante servicio existente en vez de introducir motor de datos propio.

**Comprobar:** SQL con escritura o consulta que consume demasiado; el motor remoto debe aplicar los límites incluso si el modelo los ignora.

### 23. CopilotKit — valor medio como experiencia

**Agregar:** tarjetas de progreso, formularios de confirmación y resultados estructurados en conversación.

**Hacerlo:** definir contenido y acciones en el protocolo del host; renderizar con componentes nativos de iOS, Android, escritorio y texto navegable en TUI. La biblioteca React solo sería aplicable en escritorio/web y no debe llevar lógica compartida.

**Comprobar:** aprobación por teclado y lector de pantalla; cliente antiguo muestra una representación de reserva.

### 24. Semantic Kernel — valor medio, contratos de plugins

**Agregar:** catálogo con nombre, versión, esquema, permisos y estado real de herramientas.

**Hacerlo:** ampliar datos observados de conexiones MCP, mantener firma estable para aprobar argumentos y comprobar compatibilidad de esquema; estudiar SDK como referencia en lugar de duplicar el bus ACP.

**Comprobar:** cambio de versión invalida la aprobación cuando cambie la identidad efectiva; herramienta eliminada no se ejecuta.

## Voz, aislamiento y observabilidad

### 25. LiveKit Agents — valor alto solo para telefonía

**Agregar:** llamadas entrantes y salientes, transferencia humana y mediciones de latencia cuando exista una necesidad de telefonía.

**Hacerlo:** comparar primero con `host/src/voice.rs` y la voz existente; prototipo separado con sesión LiveKit y conexión SIP autorizada. La [documentación oficial](https://docs.livekit.io/intro/basics/agents/) describe participantes de sala y procesamiento de medios; SIP, infraestructura, modelos y tarifas dependen de configuración. El host debe controlar credenciales, estado y autorización.

**Comprobar:** interrupción de voz, desconexión, transferencia, coste y consentimiento para grabar. Medir latencia real: no prometer menos de 500 ms.

### 26. Pipecat — valor medio, comparación de canalizaciones

**Agregar:** medición separada de transcripción, modelo y síntesis, y cancelación rápida al interrumpir.

**Hacerlo:** prototipo con proveedor local o externo configurado, exponiendo eventos comunes a la voz existente; mantener esta dependencia opcional y gestionar secretos en host.

**Comprobar:** audio interrumpido, proveedor sin respuesta, desconexión y eliminación de buffers al terminar.

### 27. MCP Servers — valor alto, catálogo verificable

**Agregar:** catálogo de servidores con versión, fuente, proceso, permisos y comprobación de salud; plantillas de configuración revisables.

**Hacerlo:** extender `host/src/mcp/` y el marketplace existentes; fijar paquetes concretos y sus sumas/versiones, pasar llamadas por el proxy de políticas cuando sea aplicable. Revisar el [repositorio oficial](https://github.com/modelcontextprotocol/servers) por servidor; no asumir que todos los conectores históricos siguen allí.

**Comprobar:** instalación ausente, binario cambiado, metadatos `readOnlyHint` engañosos y revocación durante una solicitud.

### 28. E2B — valor alto para ejecución aislada

**Agregar:** opción de ejecutar código generado en entorno efímero con límites de tiempo, CPU/memoria, red y exportación.

**Hacerlo:** introducir un contrato de ejecutor en host y comparar proveedor remoto E2B con aislamiento local disponible en cada plataforma; definir cómo entran archivos y secretos. No llamar sandbox al simple lanzamiento de un subprocess.

**Comprobar:** salida de red prohibida, agotamiento de memoria, timeout, archivos fuera del área permitida y destrucción de entorno. Aclarar si los datos salen del equipo.

### 29. Langfuse — valor alto para diagnósticos opcionales

**Agregar:** correlación de turno, delegación y herramienta, con duración y coste solo cuando los datos estén disponibles.

**Hacerlo:** eventos estructurados del host con ID opaco y exportador desactivado por defecto; permitir destino autoalojado e indicar qué campos salen. No afirmar que ACP expone todos los tokens o el razonamiento.

**Comprobar:** trazas incompletas, proveedor caído, redacción de secretos y desactivación sin bloquear el turno.

### 30. Phoenix — valor alto para evaluación de recuperación

**Agregar:** conjunto de preguntas y fuentes esperadas para medir calidad de búsquedas y citas.

**Hacerlo:** exportar casos sintéticos a un proceso de evaluación externo; registrar versión de corpus y configuración para comparar. Evitar exportar documentos empresariales sin permiso explícito.

**Comprobar:** documento correcto fuera del ranking, cita inexistente y cambios de índice; presentar resultados agregados reproducibles.

### 31. Vanna — valor medio, solo con caso SQL

**Agregar:** propuesta de consulta con explicación, vista previa y tabla limitada; separar generar de ejecutar.

**Hacerlo:** herramienta opcional conectada a una réplica con cuenta de solo lectura y límites impuestos por base/proxy. El host confirma base, credencial, SQL y operación antes de ejecución.

**Comprobar:** escritura disfrazada, tiempo de consulta, tablas no autorizadas y error del esquema; no mostrar resultados inventados.

### 32. OpenBB — valor bajo, datos financieros especializados

**Agregar:** conector opcional de datos de mercado con fecha, proveedor, moneda y unidades visibles.

**Hacerlo:** servicio externo o herramienta propia sobre API autorizada; guardar procedencia con cada resultado y revisar términos de redistribución del proveedor de datos.

**Comprobar:** serie sin actualizar, símbolo ambiguo, moneda distinta y proveedor caído.

### 33. LocalAI — valor medio para despliegue privado

**Agregar:** configuración de endpoint local y diagnóstico de capacidades de modelo, si el harness elegido soporta ese proveedor.

**Hacerlo:** permitir endpoint configurable en el agente compatible sin guardar claves en el cliente; mostrar si la inferencia ocurre localmente o en una dirección remota. Comprobar configuración específica antes de afirmar funcionamiento sin red.

**Comprobar:** servidor ausente, modelo no cargado, GPU insuficiente y aislamiento de red durante una sesión.

## Código como acción y dispositivos móviles

### 34. smolagents — valor medio como herramienta experimental

**Agregar:** acción de análisis aislado con código y salidas revisables, sin asumir que código generado es confiable.

**Hacerlo:** usar el contrato de ejecutor de la ficha E2B; estudiar adaptador de Python separado solo si añade capacidades medibles frente al harness existente.

**Comprobar:** importación prohibida, archivo fuera del área asignada, bucle y salida sobredimensionada.

### 35. TaskWeaver — valor medio para tablas

**Agregar:** referencia a datasets y resultados tabulares sin enviar todo el archivo al contexto del modelo.

**Hacerlo:** guardar archivos en espacio de tarea; entregar identificadores opacos y vistas limitadas a un proceso aislado; asociar versión de datos y código con cada resultado.

**Comprobar:** archivo grande, fuente cambiada durante cálculo y exportación a ubicación no autorizada.

### 36. AppAgent — valor bajo, investigación de móvil

**Agregar:** banco de pruebas para automatización de apps Android en dispositivo o emulador que el propietario haya autorizado.

**Hacerlo:** prototipo independiente que capture acción y pantalla de prueba; solo plantear integración después de definir permisos, conectividad y soporte por plataforma. No presentar control de iPhone como disponible.

**Comprobar:** resolución distinta, pantalla de autorización y aplicación bloqueada; detener ante acción incierta.

### 37. MobileAgent — valor bajo, benchmark visual

**Agregar:** comparar robustez ante cambios de interfaz y recuperación después de un gesto fallido.

**Hacerlo:** ejecutar pruebas de investigación en entornos controlados y documentar qué versión de modelo y licencia se evaluaron; usar resultados para decidir si existe demanda real.

**Comprobar:** orientación, idioma, cuadro modal inesperado y botón ausente.

## Pipelines, interfaces y evaluación

### 38. LlamaIndex — valor medio a alto para documentos

**Agregar:** ingestión incremental con citas, estado de indexación y borrado propagado.

**Hacerlo:** definir contrato de indexación y extracción de host, con servicio opcional elegido por usuario; guardar identidad de fuente y versión, y comprobar permisos al devolver cada resultado.

**Comprobar:** actualización durante indexación, revocación, documento eliminado y vuelta a indexar sin duplicados.

### 39. Haystack — valor medio, diseño modular

**Agregar:** etapas independientes de extracción, segmentación y recuperación que puedan compararse sin cambiar la conversación.

**Hacerlo:** diseñar interfaces y fixtures propias; ejecutar un pipeline externo solo si los beneficios justifican su proceso Python y mantenimiento.

**Comprobar:** fallo aislado por etapa, resultado vacío y evaluación comparativa con corpus fijo.

### 40. DSPy — valor medio en desarrollo

**Agregar:** evaluación reproducible de instrucciones sobre ejemplos y métricas, para decisiones de prompts y selección de modelos.

**Hacerlo:** suite offline de desarrollo con datos sintéticos y conjuntos separados de ajuste y prueba; registrar versión de modelo y coste. No añadir optimización automática a producción sin controles.

**Comprobar:** repetición en conjunto reservado y regresiones por grupo de casos; no evaluar solo los ejemplos de entrenamiento.

### 41. Instructor — valor medio, validación de extracción

**Agregar:** respuestas tipadas para extracción de facturas u otros documentos con errores claros y evidencia por campo.

**Hacerlo:** validador de esquema en host y adaptador opcional; imponer reintentos limitados y asociar cada dato a una fuente. La validez de formato no confirma que el importe sea correcto.

**Comprobar:** campo faltante, importe incompatible, moneda desconocida y dato sin evidencia.

### 42. AnythingLLM — valor medio, gestión de documentos

**Agregar:** espacio documental por bot con estado de carga, permisos y eliminación explícita.

**Hacerlo:** comparar con `host/src/computer/workspace.rs` y la memoria de archivos actual antes de crear un repositorio nuevo; definir aislamiento y límites de almacenamiento.

**Comprobar:** dos bots con ámbitos distintos, eliminación de archivo y búsqueda posterior sin residuos visibles.

### 43. LibreChat — valor medio, experiencia de conversación

**Agregar:** mensajes con resultado estructurado, archivos y estado claro de herramienta, manteniendo historial consistente.

**Hacerlo:** extender eventos y modelos de host de forma aditiva; representar en los cuatro clientes y conservar una versión de texto para clientes antiguos.

**Comprobar:** reconexión durante herramienta, pantalla pequeña, navegación TUI y versión antigua del cliente.

### 44. Promptfoo — valor alto, pruebas de seguridad

**Agregar:** batería repetible de inyección de instrucciones en documentos, resultados web y herramientas, con métricas por versión.

**Hacerlo:** ejecutar herramienta de pruebas fuera del producto, contra un fixture ACP local y datos sintéticos; comprobar tanto salida del agente como llamadas realmente realizadas. El equipo debe validar licencia y versión antes de incorporarla a CI.

**Comprobar:** un documento malicioso solicita exfiltrar un secreto o ignorar aprobación; el fixture comprueba que no se despachó la herramienta.

### 45. AgentOps — valor medio para diagnóstico

**Agregar:** advertencia por acciones repetidas y presupuesto de tiempo/coste cuando la fuente facilite esos datos.

**Hacerlo:** detector en host que agrupe operaciones por identidad y resultado, con umbral configurable y opción de detener o pedir ayuda; coste desconocido se presenta como desconocido.

**Comprobar:** repetición legítima con progreso no se bloquea de forma ciega; bucle sin progreso se detiene y explica el motivo.

### 46. OpenHands — valor medio para agentes de ingeniería

**Agregar:** vista de cambios y pruebas realizadas por tarea, junto con entorno de trabajo acotado.

**Hacerlo:** investigar si una versión concreta ofrece interfaz compatible con ACP; si no, explorar proceso separado sin asumir compatibilidad. Reusar vistas y permisos de host, y mostrar si el aislamiento del proyecto es real o solo una carpeta.

**Comprobar:** cambios fuera del ámbito, pruebas fallidas, interrupción y recuperación de resultados parciales.

### 47. SWE-agent — valor medio como benchmark

**Agregar:** evaluación con issues y repositorios de prueba para medir si los agentes reparan, ejecutan pruebas y explican límites.

**Hacerlo:** preparar entorno efímero con versiones fijadas y conjunto de incidencias autorizado; medir pruebas pasadas y cambios, no solo el texto final.

**Comprobar:** caso sin solución, pruebas que fallan y parche que modifica archivos no relacionados.

### 48. Swarm — valor bajo, referencia histórica

**Agregar:** pruebas de traspaso que delimiten qué contexto recibe el siguiente bot y quién conserva la responsabilidad de responder.

**Hacerlo:** adaptar los escenarios a los contratos `ask_bot` y `message_bot`; no incorporar Swarm como runtime de producción, pues su [repositorio](https://github.com/openai/swarm) lo presenta como educativo y señala un sucesor.

**Comprobar:** el bot receptor no recibe secretos ajenos; límite de handoffs y cancelación funcionan.

### 49. ascii-art — valor bajo, mejora visual optativa

**Agregar:** avatares de puntos estáticos y variante animada opcional para bots, sin que el estado dependa del color o movimiento.

**Hacerlo:** generar recursos previamente o implementar un algoritmo propio; revisar licencia y recursos concretos antes de reutilizar. iOS, Android y escritorio deben tener equivalente; TUI recibe símbolo/texto accesible.

**Comprobar:** contraste, tamaños pequeños, movimiento reducido, consumo y representaciones alternativas.

### 50. Rerun — valor alto como comparación de producto

**Agregar:** tablero de tareas por bot, plantillas de especialistas y relevo humano de tareas que utilizan la computadora.

**Hacerlo:** aprovechar `manage` para bots y `host/src/routines/` para programación; modelar tareas y dependencias en host antes de crear una vista. Conservar estados de interrupción y aprobación existentes. El [README](https://github.com/rerun-build/rerun) declara AGPL-3.0 y distingue autoalojado de funciones de nube: no copiar código sin revisión de obligaciones.

**Comprobar:** bot ocupado, tarea cancelada, permiso revocado, relevo humano y restauración de estado tras reinicio.

## Orden propuesto y criterios de decisión

1. **Instrumentación y seguridad primero:** Promptfoo, catálogo MCP y correlación optativa; los resultados deben confirmar límites reales del proxy y qué datos expone ACP.
2. **Navegador con recuperación humana:** comparar Browser Use/UI-TARS con el `computer` existente; iniciar con un entorno ficticio y medir éxito, privacidad, instalación y soporte multiplataforma.
3. **Documentos con fuentes:** probar Crawl4AI y un proveedor documental sobre un corpus sintético; medir citas correctas, ACL revocadas y latencia. No añadir varios motores sin ventaja medible.
4. **Durabilidad y equipos:** prototipo de etapas inspirado en LangGraph/Rerun, condicionado a no repetir efectos inciertos y a justificar su diferencia con rutinas y colaboración existentes.
5. **Sandbox y SQL:** hacer un modelo de amenazas y pruebas en cada sistema operativo antes de ejecutar código o conectar datos reales; priorizarlo si se requiere análisis empresarial.
6. **Telefonía, gráficos financieros y móvil:** experimentar después de existir un caso de usuario concreto, porque agregan infraestructura, coste y mantenimiento.

**Pendiente de profundidad:** las 50 fichas aportan una dirección y un experimento; no cumplen aún las 15–20 propuestas verificadas por repositorio, ni una auditoría completa de los documentos locales, código fuente, versiones, licencias y compatibilidad. Ninguna de estas propuestas está implementada o validada en aplicaciones reales.
