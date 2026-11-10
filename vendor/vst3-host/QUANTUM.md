# vst3-host 0.11.1 con un arreglo de QUANTUM

Copia de [vst3-host](https://crates.io/crates/vst3-host) 0.11.1 (licencia MIT, ver `LICENSE`) que
usa el proyecto en lugar de la publicada (`[patch.crates-io]` en el `Cargo.toml` de la raíz).

## Cambio

`src/internal/plugin_impl.rs`, `open_editor`:

- Antes, si `IPlugView::getSize` fallaba antes de colocar el editor, la ventana no se abría
  («Failed to get view size»). Kontakt 8 y otros editores de Native Instruments responden así
  hasta estar colocados. El SDK de VST3 no exige el tamaño antes de `attached`, así que ahora se
  sigue con el tamaño por defecto de la ventana.
- Después de `attached` se vuelve a pedir el tamaño; si es otro (o antes no hubo), se deja como
  pedido de cambio de tamaño y la ventana se ajusta en el siguiente `service_platform_events`.

Además, para que los plugins grandes (Kontakt) abran rápido, el editor se construye una sola vez:

- `plugin_impl.rs`, al cargar: `has_gui` sale de tener controlador, sin crear una vista de prueba.
- `window.rs`, `open`: la ventana abre con 800×600 sin pedir el tamaño con otra vista de prueba; tras
  `attached` siempre se pide ajustarla al tamaño real del editor.

Al actualizar vst3-host, comprobar si la versión nueva ya lo resuelve y, si no, repetir el cambio.
