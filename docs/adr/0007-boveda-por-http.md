# 0007. La bóveda se sirve por HTTP en 127.0.0.1

**Estado:** Aceptada

## Contexto

Las páginas necesitan vídeo y audio, y que `localStorage` se conserve entre sesiones.

## Decisión

La bóveda se sirve por HTTP en 127.0.0.1 (`http.rs`), no por un esquema propio. El puerto es fijo y se guarda en la configuración. Una clave secreta en la ruta, nueva en cada arranque, y la comprobación de `Host` frenan a otros programas locales y el DNS rebinding.

## Consecuencias

El reproductor de WebKitGTK y Media Source rechazan los esquemas propios. El origen, y con él `localStorage`, depende del puerto: por eso es fijo. Todas las bóvedas comparten ese origen, así que las páginas deben usar claves con prefijo propio.
