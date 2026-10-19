# 0007. Las bóvedas se sirven por HTTP en el bucle local

**Estado:** Aceptada

## Contexto

Las páginas necesitan vídeo y audio, y que `localStorage` se conserve entre sesiones.

## Decisión

Servidor HTTP en el bucle local (`http.rs`), no un esquema propio. Cada bóveda tiene su dirección, guardada en la configuración (`origins`): 127.0.0.1 para la que estaba abierta al llegar #39, y 127.0.0.3, .4… para las demás (ADR 0039). Los HTML sueltos se sirven en 127.0.0.2 (ADR 0019). Cada bóveda tiene también su propia clave. Una clave secreta en la ruta, nueva en cada arranque, y la comprobación de `Host` frenan a otros programas locales y el DNS rebinding. El puerto preferido se guarda en la configuración.

## Consecuencias

- El reproductor de WebKitGTK y Media Source rechazan los esquemas propios.
- El origen, y con él `localStorage`, depende del puerto: por eso se reutiliza el guardado. Si está ocupado, `http::start` toma otro y las páginas pierden su `localStorage` esa sesión.
- Cada bóveda tiene su origen, y con él su `localStorage`: una página de una bóveda no lee otra. Al llegar #39, las bóvedas que no conservaron 127.0.0.1 perdieron su `localStorage`.
