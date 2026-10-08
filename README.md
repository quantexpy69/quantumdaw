# Quantum DAW

DAW en Rust con interfaz oscura al estilo Logic. Motor lock-free con prioridad de tiempo real (PipeWire/ALSA),
regiones de audio y MIDI no destructivas, fades, time-stretch y transposición, automatización con lápiz,
grabación de audio y MIDI, QUANTUM Synth, piano roll, plugins QUANTUM (EQ, compresor, delay, reverb,
saturación y QUANTUM Tune), analizador de espectro, mixer adaptable, grupos, biblioteca con arrastrar y soltar,
importación WAV/FLAC/OGG/MP3/M4A y exportación WAV/FLAC/MP3/OGG (mezcla, una pista o stems).

## Instalar el lanzador (Fedora)

```bash
./install.sh   # compila en release e instala «Quantum DAW» en Actividades (sin sudo)
```

Los proyectos se guardan en `~/Documentos/Quantum DAW/` (o donde elijas en Archivo → Nuevo / Guardar como);
la app reabre el último proyecto. Preferencias en `~/.config/quantum-daw/config.json`.

## Uso

```bash
source env.sh                     # toolchain de Rust local en .toolchain/
cargo run --release               # abre/crea projects/Demo
cargo run --release -- <carpeta>  # abre otro proyecto
cargo test                        # prueba de exportación en todos los formatos
```

| Acción | Atajo |
|---|---|
| Play / pausa · inicio · final | `Espacio` · `Inicio` · `Fin` |
| Grabar (pistas armadas con R) | `Ctrl+R` |
| Loop · metrónomo · snap | `L` · `K` · `N` |
| Selección · lápiz (automatización) · cuchilla | `V` · `P` · `B` |
| Mostrar automatización · mixer · biblioteca | `A` · `X` · `Y` |
| Dividir en el cursor | `S` |
| Cortar / copiar / pegar / eliminar | `Ctrl+X` / `Ctrl+C` / `Ctrl+V` / `Supr` |
| Deshacer / rehacer | `Ctrl+Z` / `Ctrl+Shift+Z` |
| Nueva pista · importar · exportar | `Ctrl+T` · `Ctrl+I` · `Ctrl+E` |
| Guardar · guardar como | `Ctrl+S` · `Ctrl+Shift+S` |
| Zoom / scroll vertical / horizontal | `Ctrl`+rueda / rueda / `Shift`+rueda |

En el timeline: arrastrar en la regla define el rango de loop; arrastrar en una zona vacía crea una
selección de tiempo (sirve para cortar/copiar/exportar); doble clic bajo las pistas crea una pista nueva.
Clic derecho en una cabecera o canal: efectos, duplicar, mover, grupos, borrar automatización y eliminar.
Arrastra una cabecera (o el asa superior de un canal del mixer) para reordenar pistas. Con el lápiz,
clic derecho sobre un punto lo borra. Efectos y archivos se arrastran desde la Biblioteca.

Doble clic en un carril MIDI crea una región; doble clic en la región la abre en el piano roll.
Los puntos blancos de la barra de título de cada región ajustan los fades; arrastrar los bordes recorta y
Alt + borde derecho estira el tiempo. R e I actúan sobre todas las pistas seleccionadas.

## Estructura

- `crates/engine` — motor en tiempo real, efectos y QUANTUM Tune (`fx.rs`), sintetizador (`synth.rs`), time-stretch (`dsp.rs`), dispositivos PipeWire/MIDI (`devices.rs`) y decodificación (`io.rs`).
- `crates/project` — formato `.qproj` (v2, migra v1), proyecto demo y exportación (`export.rs`).
- `crates/app` — interfaz egui/glow: `timeline.rs`, `mixer.rs`, `pianoroll.rs`, `plugins.rs`, `analyzer.rs`, `library.rs`, `chrome.rs`.

Todo el toolchain vive en `.toolchain/` (Rust vía rustup + cabeceras de `alsa-lib-devel` extraídas sin sudo).

## Hoja de ruta

Carga de plugins VST3/LV2/CLAP en las pistas · Flex Time y Flex Pitch (edición manual de tono por nota) ·
Chord ID y pista de acordes · Session Players · Smart Tempo · Mastering Assistant · Stem Splitter ·
Live Loops · Track Stacks, envíos y buses · Spatial Audio (Dolby Atmos / ADM).
