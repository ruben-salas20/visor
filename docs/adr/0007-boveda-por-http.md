# 0007. La bóveda se sirve por HTTP en 127.0.0.1

**Estado:** Aceptada

## Contexto

Las páginas necesitan vídeo y audio, y que `localStorage` se conserve entre sesiones.

## Decisión

Servidor HTTP en 127.0.0.1 (`http.rs`), no un esquema propio. Los HTML sueltos se sirven en 127.0.0.2 (ADR 0019). Una clave secreta en la ruta, nueva en cada arranque, y la comprobación de `Host` frenan a otros programas locales y el DNS rebinding. El puerto preferido se guarda en la configuración.

## Consecuencias

- El reproductor de WebKitGTK y Media Source rechazan los esquemas propios.
- El origen, y con él `localStorage`, depende del puerto: por eso se reutiliza el guardado. Si está ocupado, `http::start` toma otro y las páginas pierden su `localStorage` esa sesión.
- Todas las bóvedas comparten origen: las páginas deben usar claves con prefijo propio.
