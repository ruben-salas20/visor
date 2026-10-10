# 0033. webkit2gtk en la misma versión exacta que wry

**Estado:** Aceptada · **Fecha:** 2026-10-09

## Decisión

`Cargo.toml` fija `webkit2gtk = "=2.0.2"`, la que exige wry 0.57. visor usa `webkit2gtk` directamente: toma el `WebView` que da wry (`view.webview()`) para el zoom, la visibilidad y señales que wry no expone. Con dos versiones del crate esos tipos no coincidirían y no compilaría.

Al actualizar wry, revisa qué `webkit2gtk` exige y cambia el pin a la misma versión.

## Cifras

2026-10-09: 2.0.2 es la última versión publicada de `webkit2gtk` (2025-12-16); no hay una posterior que probar.
