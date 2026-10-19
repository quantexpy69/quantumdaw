# QUANTUM DAW

**Estación de audio digital moderna, escrita 100 % en Rust**, con interfaz azul marino y acentos cian,
motor de audio en tiempo real sin bloqueos y herramientas profesionales de grabación, edición y mezcla.

Desarrollo: **Ivan Cheaib — QUANTUM DAW** · [www.quantumdaw.com](https://www.quantumdaw.com) · [www.quantex.com.py](https://www.quantex.com.py)

Versión actual: **0.33.0** · Linux (Fedora, PipeWire/ALSA) · Interfaz en español, inglés y portugués.

---

## Características

### Grabación y edición
- Pistas de audio mono/estéreo, MIDI, clic (metrónomo como audio) y video, hasta 128 pistas.
- Grabación de audio y MIDI con forma de onda en vivo, tomas múltiples y **comping** estilo Reaper.
- Regiones no destructivas con fades, recorte, **cuchilla**, **razor edits** sobre varias pistas,
  cortar/copiar/pegar, duplicar, normalizar, invertir, cuantizar y deshacer/rehacer ilimitado.
- **Time-stretch** y **transposición** de regiones, pistas o grupos.
- **Mapa de tempo** con cambios por tramo, metrónomo que lo sigue y **marcas** en la línea de tiempo (tecla `M`).
- **Grabación retrospectiva** (`Shift+R`): recupera en las pistas armadas el MIDI y el audio tocados en los
  últimos 30 s aunque no se estuviera grabando.
- **Edición de audio por cortes** (menú de la región): quitar silencios, cerrar huecos, cortar en los
  transientes y cuantizar audio a la rejilla, sin tocar los archivos.
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
- **QUANTUM Curves**: EQ de 7 bandas con analizador en vivo, nodos arrastrables, detección automática de
  frecuencias molestas, supresor dinámico de resonancias, **sidechain** (otra pista le abre espacio en sus
  frecuencias), curvas por banda, vúmetros IN/OUT y presets (batería, guitarras, voces, coros, synth).
- EQ de 3 bandas paramétrico, compresor (ataque, release, rodilla, mezcla paralela), delay (ping-pong,
  filtros, sincronía al tempo), reverb (pre-delay, anchura), saturación (suave, dura, válvula) y
  **QUANTUM Tune** (afinación automática o manual de voces).
- **Limitador** con anticipación, **puerta de ruido**, **chorus** y **de-esser**.
- **Mastering y loudness** (Ver → Mastering y loudness…): LUFS integrado, de corto plazo y momentáneo
  (BS.1770), true peak, rango (LRA) y correlación de fase; objetivos de Spotify, YouTube, Apple Music,
  Amazon, Tidal, Deezer, SoundCloud y EBU R128 con consejos; **cadena de efectos del master** y cadena
  de mastering sugerida.
- Perillas tipo consola con color por plugin, visor gráfico y vista previa al pasar el ratón en la biblioteca.

### Instrumentos y MIDI
- Piano roll con teclado musical en el PC, entrada desde controladores MIDI y **QUANTUM Synth**.
- Carril de control bajo el piano roll con barras verticales por nota: **velocidad**, **ganancia** (dB) y
  **ataque** (ms); se pinta arrastrando sobre varias notas y su alto se ajusta con el separador.
- En el piano roll: doble clic crea una nota, arrastrar en vacío selecciona con un recuadro (Shift suma,
  Ctrl+A todas) y cada nota se estira o acorta desde cualquiera de sus dos bordes.
- Copiar, cortar y pegar notas con Ctrl/Cmd+C, X y V (se pegan en el cursor).
- **Acordes**: el piano roll muestra el acorde que suena o que está seleccionado; modo «Crear acordes»
  (mayor, menor, 7, maj7, sus, dim… con inversiones); **círculo de quintas** con los acordes de la
  tonalidad y **progresiones automáticas** (pop, años 50, jazz, Canon…) en bloque, negras o arpegio.
- Al elegir un instrumento se abre su ventana (QUANTUM Synth con sus perillas o instrumento real).
- **QUANTUM Partitura** (Ver o Ctrl+K): editor de partituras propio en su ventana (separable a otra pantalla).
  Escribe con el ratón o el teclado (A–G, figuras 3–7, puntillo, silencios), sube y baja alturas, borra y
  deshace; claves de sol, fa, do en 3.ª y 4.ª, sol octava baja y percusión; armadura, compás, tempo e
  instrumentos transpositores. **Escribir tocando**: con un teclado MIDI o con audio (una nota a la vez)
  detecta el pulso y el tempo, cuantiza a figuras y escribe la partitura mientras tocas. Imprimir o PDF,
  MusicXML (MuseScore, Sibelius, Finale, Dorico) e intercambio con pistas MIDI.
- **Partitura** (pestaña PARTITURA): las pistas MIDI en pentagramas alineados con llave, clave de sol o
  de fa según el registro, pentagrama de percusión para la batería, instrumentos transpositores escritos
  como los leen sus intérpretes (clarinete, trompeta, saxos, trompa), armadura detectada, cifrado de
  acordes encima, e impresión o PDF en A4 (Ctrl+P). Usa la fuente musical Bravura (SIL OFL).
- **Inspector** (tecla I), **cabeceras de pista** compactas, **menús circulares** (clic derecho o Q sobre una
  región o en el piano roll), **secuenciador de patrones** con **caja de ritmos** sintetizada y **escenario
  de clips** (Clip Stage) con escenas que se lanzan en bucle desde el próximo compás.
- **Generador de progresiones** (botón «Generar…» del piano roll o Ctrl+K): 15 estilos (Cinematic, Ambient,
  Indie Pop, Emo, Pop, Lo-fi/Neo Soul, Jazz, R&B/Gospel, Rock, Synthwave, EDM/Trance, Dark/Horror,
  Folk/Country, Blues, Heroic/Game), tonalidad, largo, velocidad, color, variación y bajo caminante;
  se escucha y se aplica a la pista de acordes y, si se quiere, como notas.
- **Herramientas MIDI**: bloqueo de escala (24 escalas y modos), arpegiador de la selección (arriba,
  abajo, arriba y abajo, aleatorio en 1/8, 1/16 o 1/32), rasgueo (strum) y generador de melodías en la
  escala; 13 estilos de progresión, incluido el blues de 12 compases.
- Instrumentos reales gratuitos y libres descargables (guitarras, bajos, batería, cuerdas, piano),
  con su licencia indicada en cada uno, y opción de desinstalarlos.

### Proyecto e interfaz
- Biblioteca con archivos (discos y carpetas), efectos, instrumentos y plugins VST3/LV2/CLAP instalados.
- **Pista de video** siempre arriba: la región lleva la imagen y su audio juntos, se mueve, corta y recorta
  como cualquier región; doble clic abre el visor con controles de reproducción.
- Importación WAV/FLAC/OGG/MP3/M4A y video (vía ffmpeg) sincronizado; exportación WAV/FLAC/MP3/OGG
  de la mezcla, de una pista o en stems.
- **Pantalla inicial** con saludo, continuar, demo, plantillas, recientes, inicio rápido, espacios de trabajo,
  **novedades del blog** y **tienda QUANTUM** de quantumdaw.com (con caché sin conexión) e instrumentos gratis.
- **Proyectos que agrupan canciones** (por ejemplo un álbum): cada proyecto tiene su ficha
  (`proyecto.json`) y cada canción guarda quién la creó, fecha y hora de creación y del último guardado,
  formato (frecuencia y bits), tempo, métrica y número de pistas.
- **Ventanas independientes**: mixer, biblioteca, plugins, ruteo y video pueden flotar dentro del programa o
  separarse como ventanas del sistema (por ejemplo en otro monitor) y volver a acoplarse.
- Atajos de teclado y de rueda del ratón configurables, analizador de espectro e interfaz escalable.
- **Paleta de comandos** (Ctrl/Cmd+K): busca cualquier acción por su nombre y la ejecuta con Enter.
- **Temas de color**: Marino (predeterminado: azul marino con acento cian), Pizarra (azul grisáceo con
  acento naranja), Grafito, Medianoche, Carbón, Ciruela, Bosque y Ámbar (Ver → Tema).
- **Autoguardado** cada minuto con cambios y recuperación tras un cierre inesperado.
- **Importar y exportar MIDI** (.mid): cada pista del archivo se convierte en una pista MIDI; se exporta
  una región o todas las pistas MIDI.
- **Informes de error**: registro en la carpeta de registros, informe automático si el programa se cierra
  por un fallo o deja de responder (con la pila de llamadas de todos los hilos) y envío opcional a
  quantumdaw.com; también desde **Herramientas → Enviar informe de error…**.
- La configuración de audio (dispositivos, frecuencia y buffer) se cambia en cualquier momento con el
  proyecto abierto; si el dispositivo nuevo no abre, vuelve al anterior.

---

## Instalación en Windows y macOS

- **Windows 10 y 11 (64 bits):** `quantum-daw-windows-x64-setup.exe` (instalador) o `quantum-daw-windows-x64.zip` (portable).
  Para exportar MP3 en Windows instala ffmpeg (`winget install ffmpeg`).
- **macOS 11 Big Sur o posterior (Intel y Apple Silicon):** `quantum-daw-macos-universal.dmg`; arrastra Quantum DAW a
  Aplicaciones. La primera vez ábrelo con clic derecho → Abrir (la app no está notarizada por Apple).

## Cómo se guarda cada canción

Cada canción es una carpeta autocontenida (por defecto en `~/Documentos/Quantum DAW/`, opcionalmente dentro de
un proyecto que agrupa varias canciones):

```
Mi canción/
├── project.qproj   pistas, regiones, mezcla, efectos y sus ajustes, automatización, tempo, marcas, acordes…
├── Audio Files/    grabaciones e importaciones, siempre en WAV a la frecuencia del proyecto
├── Proxies/        render previo de las pistas pesadas (modo «Trabajar con proxys»)
├── Video/          videos importados y sus fotogramas en caché
├── Rendered/       exportaciones (mezcla, pistas y stems)
└── Backups/        copias automáticas de project.qproj en cada guardado (las 20 últimas)
```

Los instrumentos descargables se guardan una sola vez para todas las canciones (en `~/.local/share/quantum-daw`,
`~/Library/Application Support/Quantum DAW` o `%LOCALAPPDATA%\Quantum DAW`).

## Instalación en Linux

Descarga la última versión desde [Releases](https://github.com/quantexpy69/quantumdaw/releases/latest).
Funciona en cualquier distribución x86_64 con glibc 2.28 o posterior.

| Distribución | Archivo | Instalación |
|---|---|---|
| Debian, Ubuntu, Linux Mint, Pop!_OS… | `quantum-daw-amd64.deb` | `sudo apt install ./quantum-daw-amd64.deb` |
| Fedora, RHEL, CentOS Stream, Rocky Linux, AlmaLinux | `quantum-daw-x86_64.rpm` | `sudo dnf install ./quantum-daw-x86_64.rpm` |
| openSUSE | `quantum-daw-x86_64.rpm` | `sudo zypper install ./quantum-daw-x86_64.rpm` |
| Arch Linux, Manjaro, EndeavourOS | `PKGBUILD` | `makepkg -si` |
| Cualquiera (portable) | `quantum-daw-x86_64.AppImage` | `chmod +x` y ejecutar |
| Cualquiera (sin sudo) | `quantum-daw-linux-x86_64.tar.gz` | descomprimir y `./instalar.sh` |

Opcional: `ffmpeg` (importar video), `7z` y `curl` (instrumentos descargables).

### Compilar desde el código

```bash
git clone https://github.com/quantexpy69/quantumdaw.git
cd quantumdaw
./install.sh   # instala Rust en .toolchain/ si hace falta y te indica las cabeceras de ALSA de tu distribución
```

Para crear todos los paquetes (.deb, .rpm, AppImage, .tar.gz y PKGBUILD) con podman o docker:
`packaging/build.sh` (compila dentro de AlmaLinux 8 para que funcionen en todas las distribuciones).
Al subir una etiqueta `vX.Y.Z`, GitHub Actions publica el release con sus notas y todos los paquetes.

Los proyectos se guardan en `~/Documentos/Quantum DAW/` y las preferencias en `~/.config/quantum-daw/config.json`.
Los registros y los informes de error quedan en `~/.local/share/quantum-daw/registros/`.

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

© 2026 Ivan Cheaib — QUANTUM DAW. Código bajo licencia [MPL-2.0](LICENSE). Los instrumentos de terceros conservan sus propias licencias; la fuente musical Bravura es de Steinberg bajo SIL Open Font License 1.1 (`assets/fuentes/OFL-Bravura.txt`).
