# QUANTUM DAW

**Estación de audio digital moderna, escrita 100 % en Rust**, con interfaz oscura de alto contraste,
motor de audio en tiempo real sin bloqueos y herramientas profesionales de grabación, edición y mezcla.

Desarrollo: **Ivan Cheaib — QUANTUM DAW** · [www.quantumdaw.com](https://www.quantumdaw.com) · [www.quantex.com.py](https://www.quantex.com.py)

Versión actual: **0.5.3** · Linux (Fedora, PipeWire/ALSA) · Interfaz en español, inglés y portugués.

---

## Características

### Grabación y edición
- Pistas de audio mono/estéreo, MIDI, clic (metrónomo como audio) y video, hasta 128 pistas.
- Grabación de audio y MIDI con forma de onda en vivo, tomas múltiples y **comping** estilo Reaper.
- Regiones no destructivas con fades, recorte, **cuchilla**, **razor edits** sobre varias pistas,
  cortar/copiar/pegar, duplicar, normalizar, invertir, cuantizar y deshacer/rehacer ilimitado.
- **Time-stretch** y **transposición** de regiones, pistas o grupos.
- **Mapa de tempo** con cambios por tramo, metrónomo que lo sigue y **marcas** en la línea de tiempo (tecla `M`).
- **Pista de arreglo** (Intro, Estrofa, Pre coro, Coro, Puente…): clic en una sección para seleccionar su rango o hacer loop.
- **Pista de acordes** con selector de raíz y tipo (m, 7, maj7, sus4, dim…).
- **Contador de tempo inteligente**: TAP o detección automática del BPM del audio.

### Editor de audio (doble clic en una región)
- Selección de rangos con zoom (rueda), cortar, copiar, pegar y eliminar partes del audio (`Supr`, `Ctrl+X/C/V`).
- Normalizar, cambiar ganancia, fundidos de entrada/salida, silencio, invertir fase, invertir (reverse),
  acortar y eliminar desplazamiento de CC.
- **Time & Pitch Machine**: transposición por intervalos y cents, duración en %, BPM destino, muestras,
  SMPTE o compases, con previsualización y "Procesar y pegar".
- Ajuste de tempo por selección y localizadores, búsqueda de picos y de silencios.

### Mezcla
- Mixer adaptable con faders en dB, medidores con retención de picos, panorama, mute/solo/armado/monitoreo,
  **inversión de polaridad (Ø)**, envíos post-fader, buses y ruteo (ROUTE).
- Grupos con barra de color al estilo Ardour y edición vinculada.
- Automatización de volumen y panorama dibujada con lápiz o grabada desde el mixer con los modos
  **Lectura, Apagado, Toque, Retención y Escritura** (Read, Off, Touch, Latch y Write).
- Canales con botones **M** (rojo) y **S** (amarillo) y ranuras de **Envíos**, **Grupo** y modo de automatización.

### Plugins QUANTUM
- EQ de 3 bandas paramétrico, compresor (ataque, release, rodilla, mezcla paralela), delay (ping-pong,
  filtros, sincronía al tempo), reverb (pre-delay, anchura), saturación (suave, dura, válvula) y
  **QUANTUM Tune** (afinación automática o manual de voces).
- Perillas tipo consola con color por plugin, visor gráfico y vista previa al pasar el ratón en la biblioteca.

### Instrumentos y MIDI
- Piano roll con teclado musical en el PC, entrada desde controladores MIDI y **QUANTUM Synth**.
- Instrumentos reales gratuitos y libres descargables (guitarras, bajos, batería, cuerdas, piano),
  con su licencia indicada en cada uno, y opción de desinstalarlos.

### Proyecto e interfaz
- Biblioteca con archivos (discos y carpetas), efectos, instrumentos y plugins VST3/LV2/CLAP instalados.
- **Pista de video** siempre arriba: la región lleva la imagen y su audio juntos, se mueve, corta y recorta
  como cualquier región; doble clic abre el visor con controles de reproducción.
- Importación WAV/FLAC/OGG/MP3/M4A y video (vía ffmpeg) sincronizado; exportación WAV/FLAC/MP3/OGG
  de la mezcla, de una pista o en stems.
- Menú principal con nueva canción, datos de la canción y copyright, recientes y demo.
- **Proyectos que agrupan canciones** (por ejemplo un álbum): cada proyecto tiene su ficha
  (`proyecto.json`) y cada canción guarda quién la creó, fecha y hora de creación y del último guardado,
  formato (frecuencia y bits), tempo, métrica y número de pistas.
- Atajos de teclado y de rueda del ratón configurables, analizador de espectro e interfaz escalable.

---

## Instalación (Fedora)

```bash
git clone https://github.com/quantexpy69/quantumdaw.git
cd quantumdaw
./install.sh   # compila en release e instala «Quantum DAW» en Actividades (sin sudo)
```

Requisitos: Rust (o el toolchain local en `.toolchain/`, ver `env.sh`), cabeceras de ALSA y, para
video, `ffmpeg`/`ffprobe`. Los proyectos se guardan en `~/Documentos/Quantum DAW/` y las preferencias
en `~/.config/quantum-daw/config.json`.

## Desarrollo

```bash
source env.sh            # toolchain de Rust local
cargo run --release      # abre el último proyecto (o la demo)
cargo test               # pruebas del motor, exportación, procesos e idiomas
```

| Acción | Atajo |
|---|---|
| Reproducir / pausa · inicio · final | `Espacio` · `Inicio` · `Fin` |
| Grabar en pistas armadas | `Ctrl+R` |
| Loop · metrónomo · snap · marca | `L` · `K` · `N` · `M` |
| Selección · lápiz · cuchilla | `V` · `P` · `B` |
| Automatización · mixer · biblioteca | `A` · `X` · `Y` |
| Dividir en el cursor | `S` |
| Cortar / copiar / pegar / eliminar | `Ctrl+X` / `Ctrl+C` / `Ctrl+V` / `Supr` |
| Deshacer / rehacer | `Ctrl+Z` / `Ctrl+Shift+Z` |
| Nueva pista · importar · exportar | `Ctrl+T` · `Ctrl+I` · `Ctrl+E` |
| Guardar · guardar como | `Ctrl+S` · `Ctrl+Shift+S` |

Todos los atajos se cambian en **Herramientas → Atajos de teclado y ratón**.

## Estructura

- `crates/engine` — motor en tiempo real (cpal, lock-free), efectos, sintetizador, sampler SFZ,
  time-stretch, detección de tempo, mapa de tempo y dispositivos.
- `crates/project` — formato de proyecto `.qproj` (JSON), demo y exportación.
- `crates/app` — interfaz egui: timeline, mixer, piano roll, editor de audio, plugins, biblioteca,
  menú principal e idiomas (`i18n.tsv`).

## Hoja de ruta

Carga de plugins VST3/LV2/CLAP en las pistas · compensación de latencia · Flex Time / Flex Pitch ·
pista de acordes · Mastering Assistant · separación de stems · Spatial Audio.

---

© 2026 Ivan Cheaib — QUANTUM DAW. Los instrumentos de terceros conservan sus propias licencias.
