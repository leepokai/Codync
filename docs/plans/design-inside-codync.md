# Plan: diseño dentro de la app (Penpot embutido)

Fecha: 2026-11-08 · Rama candidata: una rama nueva en su propio worktree

Objetivo: que Codync tenga un editor de diseño tipo Figma dentro de la aplicación: instalar, arrancar y abrir el editor sin cuenta externa, sin configuración y sin Docker. La pieza elegida es Penpot; el resto del plan de motores (`docs/plans/bundled-tools-production.md`) no cambia.

## Principios

- El runner compartido manda: Penpot se suma como un motor más de `host/src/tools/`, no como un caso aparte.
- Un solo proceso queda visible en un puerto local; el resto vive en `127.0.0.1` con puertos que se asignan libre al arrancar.
- El programa viaja en `resources/`; los datos en `userData/penpot` y sobreviven a actualizar.
- Cero configuración: propietario sembrado, sesión automática, sin correo ni registro público.
- Sin Docker: el backend se compila desde el código fuente en CI; nada de las imágenes de Docker llega al usuario.

## Alcance

| Pieza | Cómo entra |
| --- | --- |
| Backend de Penpot | Compilado en CI a `build/native/penpot` |
| Runtime del backend | JDK en `build/native/jdk` (igual que Node para n8n) |
| Base de datos | PostgreSQL portátil en `build/native/postgres` |
| Cola y caché | Redis compilado para Windows en `build/native/redis` |
| Frontend | Archivos estáticos servidos por el backend (un puerto menos) |
| Exporter | Fuera en la primera entrega; PNG y SVG por Chromium propio de la app |
| Datos | `userData/penpot/pg` (base) y `userData/penpot/assets` (imágenes e archivos) |
| Puerto visible | Uno libre que se busca solo; el resto interno |
| Auth | Propietario sembrado con correo `local@codync` y contraseña en el vault; sin registro |

## Arquitectura

1. `host/src/tools/runner.rs` arranca en orden: Postgres, Redis, backend, con estado `stopped/starting/running/error` por pieza.
2. `health.rs` pregunta al backend (`/api/health`) con reintentos acotados; la ventana solo se abre cuando responde.
3. La ventana usa el patrón de n8n: página de espera, origen local permitido y salto al editor.
4. `lifecycle.rs` para todo al salir y al actualizar, sin huérfanos: ni Postgres ni backend quedan vivos al cerrar.
5. En el rail y en Extensiones › Apps aparece la tarjeta Diseño con Abrir, Parar, Log por pieza y espacio usado.

## Sembrar propietario y sesión

- Primer arranque: si `userData/penpot/pg` está vacía, se aplican las migraciones y se inserta el propietario con el método documentado del backend (mismo patrón que el propietario de n8n).
- Sesión: cookie propia en cada arranque; si no aparece, reintento antes de cargar el editor. Nunca se muestra un login.
- Correo desactivado. Las funciones de equipo quedan locales y se abrirán cuando el plan de servidores propios lo coja.

## Fases y aceptación

| Fase | Días | Aceptación |
| --- | --- | --- |
| 0. Excalidraw de acompañante (opcional) | 1 | Pizarra como ficha del lienzo que se guarda como documento |
| 1. Postgres y Redis empaquetados | 2 | Arrancan, salud, parada ordenada y datos que sobreviven a actualizar |
| 2. Backend compilado y sembrado | 3 | Instalar en máquina limpia sin JDK ni Postgres: el editor abre directo |
| 3. Sesión, puerto y salud | 2 | Puerto tomado se resuelve; sin cookie no hay login visible |
| 4. Datos y ciclo de vida | 2 | Respaldo antes de actualizar; desinstalar ofrece conservar |
| 5. Exporter o export por CDP | 2-3 | PNG del proyecto llega como adjunto al chat |
| 6. Matriz final | 2 | Limpio, puertos tomados, Redis caído, cerrar y reiniciar el equipo |

Total: cerca de 15 días, menos si el runner de n8n y ComfyUI ya está hecho.

## Fallos previstos

| Fallo | Respuesta |
| --- | --- |
| Puerto tomado | Probar el siguiente y decirlo en el log |
| Redis caído | Reiniciar solo Redis; el editor sigue si puede y avisa |
| Base dañada | Volver al respaldo `.zip` y avisar, sin perder nada después del último respaldo |
| Actualización con migraciones | Respaldo, migraciones del propio backend y vuelta atrás si no arranca |
| Exporter falla | Export por Chromium propio de la app (renderizar y capturar) mientras se resuelve |

## Importar y exportar

- Importar: los archivos `.figma` propios de Figma no se importan; sí SVG, y el archivo de Penpot como texto editable por el bot.
- Exportar: PNG y SVG por el exporter o por captura de la propia app.
- Puente MCP `penpot`: listar y abrir proyectos, adjuntar export y copiar los colores del tema para que los bots los usen en la app.

## Empaquetado

- `extraResources`: `build/native/penpot`, `build/native/jdk`, `build/native/postgres`, `build/native/redis`.
- Todo fuera del asar, rutas cortas y firmado como el resto. Tamaño estimado: 350 a 450 MB, bien por debajo de ComfyUI.
- CI: job que compila el backend y genera los binarios con caché por versión.

## Qué hay que evitar

Que el usuario reciba un Postgres y un Redis invisibles. El log de cada pieza, el espacio usado y la parada por pieza están visibles; cualquier reparación se ofrece desde la tarjeta Diseño, no con comandos.

## Fuera de alcance

- Edición colaborativa en vivo entre personas.
- Correo, invitaciones y registro público.
- Importar archivos de Figma tal cual.
- macOS y Linux en la primera entrega de Penpot; solo Windows, como ComfyUI.
