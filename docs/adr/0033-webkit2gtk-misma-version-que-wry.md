# 0033. webkit2gtk en la misma versión exacta que wry

**Estado:** Aceptada · **Fecha:** 2026-10-09

## Contexto

visor usa `webkit2gtk` directamente: toma el `WebView` de WebKit que le da wry (`view.webview()`) para el zoom, la visibilidad y las señales que wry no expone. wry 0.57 exige `webkit2gtk = "=2.0.2"`.

## Decisión

`Cargo.toml` fija `webkit2gtk = "=2.0.2"`, la misma versión que pide wry.

## Consecuencias

- Los tipos de WebKit que devuelve wry y los que usa visor son los mismos. Con dos versiones del crate, no lo serían y no compilaría.
- Al actualizar wry, revisa qué versión de `webkit2gtk` exige y cambia el pin a la misma.

## Cifras

- 2026-10-09: 2.0.2 es la última versión publicada de `webkit2gtk` (2025-12-16). No hay una posterior que probar.
