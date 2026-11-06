# Codync — Avatar Studio & Decision Lab (V5 · Interfaz Estilo Video, Plantillas SVG-to-Halftone, Accesorios HD y Grupos Unidos)

Prototipo estático e interactivo de un solo archivo (`index.html`) para evaluar y diseñar el sistema de avatares de bots de **Codync**, combinando la interfaz de personalización del video de referencia con nuestro motor de matriz de puntos **Halftone 2**.

## Cómo abrir el prototipo

1. Haz **doble clic** directamente sobre:
   `C:\Users\Edwin Villeda\Documents\Codync-avatar-lab\docs\design\avatar-lab\index.html`
2. No requiere servidor, compilación ni conexión a internet (cero dependencias externas).

---

## Qué incluye esta versión (V5)

### 1. Interfaz Estudio Interactivo (Inspirada en el video + Personalización Completa)
- **Columna Izquierda (Escenario en Vivo):**
  - **Píldora flotante superior** con miniatura en vivo (`26 pt`) y nombre del personaje o grupo activo + botón `🌌 Spawn`.
  - **Avatar Héroe (`185 pt`)** interactivo: sigue el cursor con la mirada y al pasar el ratón o hacer clic **reacciona feliz (`^ ^`) con salto elástico y chispas**.
  - **Píldoras de estado** debajo del avatar: `Idle` · `Thinking` (`working`) · `Happy` · `Needs Input` · `Speaking` (voz con boca de puntos lip-sync) · `Screen` (`computerUse`).
  - **Tira multi-escala en tiempo real** debajo: muestra simultáneamente cómo se ve tu personalización o grupo unido a **`46 pt` (Móvil)**, **`30 pt` (Sidebar)**, **`18 pt` (Inline)** y **`TUI` (3 columnas)**.
- **Columna Derecha (5 Pestañas + Barra de Plantillas Rápidas):**
  1. **`Personaje (18)`:** Cuadrícula visual con **18 personajes en matriz de puntos** (`Cat 🐱`, `Sprout 🌱`, `Gamepad 🎮`, `Mech 🤖`, `Mug ☕`, `Espíritu 👻`, `Burbuja 💬`, `Regadera 🚿`, `Owl 🦉`, `Fox 🦊`, `Rocket 🚀`, `Matraz 🧪`, `Blob`, `Squircle`, `Hexágono`, `Diamante`, `Estrella`, `Cloud`) que cambian de color en vivo al elegir cualquiera de los **14 círculos cromáticos** inferiores, más el botón **`+ Importar`**.
  2. **`Personalizar & Rol`:**
     - **10 Accesorios de Oficio en Puntos (`ROLE_PROPS`)** con **halo/placa de alto contraste** para que jamás se camuflen con el color del cuerpo: `👓 Lentes Analista`, `📊 Hoja de Cálculo`, `💻 Holograma </>`, `🔍 Lupa / Monóculo QA`, `🎧 Headset de Voz`, `🛡️ Escudo SecOps`, `🎨 Boina & Paleta`, `👑 Corona Líder`, `👔 Corbata Ejecutiva` y `🔧 Casco & DevOps`.
     - **Controles directos de rasgos** (100 % aplicados en tiempo real): `Estilo de Ojos` (`round`, `slit`, `wide`, `happy ^ ^`, `wink`, `visor Cyber`), `Corona Superior`, `Textura de Piel ADN`, `Esquina del Acento`, y **sliders de geometría ocular** (`Separación de ojos`, `Tamaño de ojos`, `Altura Y`) y `Densidad de Puntos` (`13×13`, `15×15`, `17×17`).
  3. **`Grupos Unidos (Huddle)`:**
     - Muestra en el escenario central (y en `46 pt / 30 pt / 18 pt`) a los equipos **realmente agrupados hombro con hombro como un solo escuadrón**, inclinados hacia el centro con superposición limpia de contorno, aura compartida y partículas sinápticas.
     - Permite ajustar en vivo la **Formación** (`Abrazo de Escuadrón / Huddle`, `Abanico Táctico en V`, `Fusión Simbiótica`) y el **slider de Cercanía / Unión del Grupo (`65%–100%`)**.
  4. **`Generate (IA)` (Herramienta interna de diseño):**
     - Incluye el generador de prompt `Avatar geometry designer` del video para crear nuevas siluetas mediante IA y **5 respuestas listas para probar con un clic** (`Dragon · Guardián`, `Astronaut · Explorador`, `Penguin · Analista Linux`, `Briefcase · Ejecutivo CFO`, `Trophy · Campeón Ventas`).
  5. **`Import / JSON`:**
     - Editor y validador en vivo del esquema JSON `version: 1` (`body.path` SVG en coordenadas `0..100` + `eyes: { x, y, spacing, scale }` + `color` + `props` + `traits`), que el motor convierte automáticamente en puntos 3D Halftone con supermuestreo 5×.
