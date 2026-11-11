# Plan: n8n y ComfyUI dentro de la app, arranque directo

Fecha: 2026-11-07 · Rama: `feat/openbot-marketplace`

Objetivo: que producción instale los dos ya dentro, los arranque sola y abra el editor directo, sin detectar instalaciones, sin pedir ajustes y sin login. Las licencias no se gestionan en este plan.

## Principios

- El programa viaja en `resources/`, no en una carpeta del usuario.
- Lo que cambia (datos, modelos, base) vive en la carpeta de usuario y sobrevive a actualizar.
- Cero configuración para el usuario: si algo falta, se arregla o se explica con una acción.
- Todo lo común a los dos va en un módulo compartido; n8n se ajusta y ComfyUI se replica sobre él.

## Alcance

| Pieza | n8n | ComfyUI |
| --- | --- | --- |
| Código del programa | Se compila en CI y se copia a `resources/n8n` | Portable oficial descomprimido a `resources/comfy` |
| Runtime | Node embebido en `resources/node` | Python y torch del portable |
| Datos | `userData/n8n` (base y archivos) | `userData/comfy` + `models/` |
| Puerto | 5678 con búsqueda de libre | 8188 con búsqueda de libre |
| Auth | Propietario sembrado + cookie de sesión | Ninguna |
| Apertura | Editor directo cuando la salud responde | Editor directo cuando la salud responde |

## Módulo compartido (`host/src/tools/`)

1. `runner.rs`: arranca un proceso hijo desde `resources`, con `current_dir`, entorno acotado, captura de log, puerto libre y estado `stopped/starting/running/error`.
2. `health.rs`: sonda propia por herramienta (`GET /healthz` en n8n, `GET /system_stats` en ComfyUI), con timeout y reintento acotado.
3. `window.rs` (main): abre la ventana, permite solo el origen local, muestra página de espera y navega sola al editor.
4. `lifecycle.rs`: arranque con el host, parada ordenada al salir y al actualizar, sin huérfanos.

n8n y ComfyUI quedan como dos configuraciones del mismo runner, no como dos implementaciones.

## A · Ajustes a n8n

1. Empaquetar: compilar el monorepo (`npm ci && npm run build`) a `build/native/n8n`, copiar Node a `build/native/node` y declarar ambos en `extraResources`.
2. Primer arranque: si `userData/n8n` está vacía, sembrar propietario (correo `local@codync`, contraseña aleatoria guardada en el vault) y ejecutar el alta sin interacción.
3. Sesión: obtener la cookie `automan-auth` en cada arranque y **reintentar** si no aparece; antes de cargar el editor, no después.
4. Salud y puerto: comprobar libre, elegir otro si está tomado y arrancar con `--port`.
5. Ventana: cargar el editor solo cuando la salud responde; si falla, página con motivo, Ver log y Reintentar.
6. Datos: mover la base y los archivos a `userData/n8n`; respaldo `.zip` antes de cada actualización.
7. Actualizar y desinstalar: reemplazo versionado con vuelta atrás; desinstalar ofrece conservar datos.
8. Log: visible en Ajustes y adjunto al reportar un fallo.

## B · Replicar en ComfyUI

1. Paquete: descargar el portable fijado en CI, descomprimir a `resources/comfy`; excluir modelos.
2. Arranque: `python/main.py --listen 127.0.0.1 --port <libre>`; sin auth.
3. Comprobaciones: espacio, GPU/driver, CPU como respaldo avisado.
4. Modelos: carpeta `userData/comfy/models` enlazada; si falta el checkpoint, mensaje con Abrir carpeta y enlace de descarga.
5. Ventana y rail: mismo patrón que n8n; tarjeta en Extensiones › Apps con Abrir, Parar, Actualizar, Modelos.
6. Cola: un trabajo a la vez, tiempo máximo y cancelación real.
7. Herramientas: puente MCP `comfyui` para listar, ejecutar, cancelar y subir imágenes; resultados como adjuntos.

## Empaquetado

- `electron-builder` `extraResources`: `build/native/n8n`, `build/native/node`, `build/native/comfy`.
- Fuera del asar, rutas largas de Windows acortadas y firmadas.
- Tamaño: n8n y Node en todas las plataformas; ComfyUI solo Windows x64 en la primera entrega.
- CI: job por plataforma con caché de n8n y del portable.

## Fallos previstos

| Fallo | Respuesta |
| --- | --- |
| Puerto ocupado | Probar el siguiente y decirlo en el log |
| Node o Python ausente | No aplica: van dentro |
| GPU sin driver | Arrancar en CPU con aviso y enlace al driver |
| Modelo ausente | Abrir carpeta y enlace; el resto de la app sigue usable |
| Proceso muerto | Estado `error`, Reintentar y reporte con log |
| Actualización | Respaldo, reemplazo y vuelta atrás si no arranca |

## Fases y aceptación

| Fase | Días | Aceptación |
| --- | --- | --- |
| 1. Runner compartido + n8n empaquetado | 3 | En máquina limpia, instalar y abrir el editor n8n sin pasos manuales |
| 2. Sesión, puerto, salud y errores de n8n | 2 | Sin cookie no hay login visible; puerto tomado se resuelve solo |
| 3. Datos, respaldo y desinstalar | 2 | Actualizar conserva la base; desinstalar deja elegir |
| 4. ComfyUI empaquetado y ventana | 3 | Instalar y ver el editor de ComfyUI abrir directo |
| 5. GPU, modelos y cola | 3 | Un trabajo genera imagen; sin GPU avisa y no bloquea |
| 6. MCP y adjuntos | 2 | Un bot genera una imagen que llega al chat y se cancela |
| 7. Matriz y cierre | 2 | Limpia, sin Node, puerto ocupado, sin GPU, AMD; todo pasa |

Total: ~17 días de trabajo efectivo.

## Pruebas obligatorias

- Windows limpio sin Node ni Python.
- Puerto 5678 y 8188 ocupados.
- Equipo sin GPU y equipo AMD.
- Actualizar desde la versión anterior sin perder datos.
- Cerrar la app con ambos corriendo: sin procesos huérfanos.
- Reinicio del equipo: ambos vuelven si estaban activos.

## Fuera de alcance

- Licencias y avisos.
- macOS y Linux para ComfyUI (segunda entrega).
- Varios trabajos de ComfyUI en paralelo.

## Otros motores: qué entra y qué no

El runner sostiene **un proceso y un puerto**; eso solo lo cumplen los motores de un solo servicio. Los que necesitan Postgres, Redis y varios workers quedan fuera del instalador.

| Sistema | Servicios | Encaja | Decisión |
| --- | --- | --- | --- |
| n8n | 1 | Sí | Dentro, ya planificado |
| Node-RED | 1 | Sí | Opcional: entra si se pide IoT/MQTT |
| ComfyUI | 1 | Sí | Dentro, ya planificado |
| InvokeAI | 1 | Sí, con mucha VRAM | Alternativa a ComfyUI, no junto |
| Flowise | 1 | Sí | Opcional para RAG visual |
| Activepieces | 2+ (Postgres, Redis) | No tal cual | Solo con Postgres empaquetado, después |
| Dify | 6+ (web, api, worker, Postgres, Redis, vector) | No | Fuera del instalador; se conecta por URL |

Regla: un motor nuevo entra si arranca como un proceso, escucha en un puerto y guarda en su carpeta de usuario. Si no, se conecta por URL en vez de empaquetarse.

