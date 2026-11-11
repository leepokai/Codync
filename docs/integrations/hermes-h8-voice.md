# H8: voz Hermes integrada en el host

Origen MIT fijado: NousResearch/hermes-agent 10d25f8ad65fdf71304e9272959421a0c22e5d23,
`tools/neutts_synth.py`, `tools/neutts_samples/jo.wav`, `jo.txt`, `hermes_cli/setup_tts.py`,
y `pyproject.toml` (extra `neutts[all]==1.4.1`). Se reutilizan el entorno y los modelos de
Hermes, sin copiar sus credenciales ni volver a autorizar conexiones.

Adaptación: `voice/neutts.rs` conserva la selección de modelo y referencia de Hermes;
`neutts_stream.py` adapta el API `NeuTTS.infer_stream` de NeuTTS 1.4.1 a paquetes PCM
acotados. `streaming.rs` supervisa el árbol del proceso, stdin privado para texto,
stdout JSON limitado y stderr descartado con límite. Caché offline, sin instalación,
Python 3.11–3.13, 80 segundos de plazo total y exclusión de síntesis simultánea.

`voiceStartSpeech`, `voiceSpeechChunk` y `voiceCancelSpeech` usan UUID por solicitante;
una cancelación que llega antes del arranque impide que este arranque después. Se
limitan solicitudes activas, tombstones, cola (tres frames), tamaño de frame (24 KB de
PCM), duración y lecturas simultáneas. Los errores finales no se confunden con EOF.
Cada frame es un WAV completo PCM16 mono 24 kHz de hasta 500 ms, por lo que los tres
reproductores existentes pueden comenzar antes de terminar la inferencia. Prefetch
cliente: un frame. Las conexiones STT del usuario se conservan y el motor TTS se elige
por separado en escritorio, iOS y Android.

Las tres implementaciones de Speech aceptan interrupción por voz sostenida y cancelan
el proceso remoto/local pendiente al interrumpir o cerrar. iOS/Android incorporan
opción OFF por defecto Hey Hermes, mediante reconocimiento nativo local en primer
plano, con ventana de un comando de 15 segundos. No se atribuye soporte al modelo
TFLite/Python de Hermes: se adapta el gesto usando reconocimiento local ya existente.
No hay escucha en segundo plano ni subida a nube para detectar la frase. TUI carece
de captura/reproducción y sigue usando texto; no se simula una capacidad de audio.

Validación: cobertura fuente añadida para cancelación aislada/anticipada, framing
truncado/excesivo y puerta de frase/timer/ciclo de vida en ambos móviles. Por petición
expresa del usuario, no se ejecutaron compilaciones, tests, proveedores, descargas ni
pruebas de audio. La revisión de fuente no sustituye la comprobación real de eco,
latencia, disponibilidad de modelos o permisos de cada sistema.
