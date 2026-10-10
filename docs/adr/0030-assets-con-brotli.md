# 0030. Librerías de assets comprimidas con brotli

**Estado:** Aceptada

## Contexto

WebKit solo anuncia gzip por HTTP, pero descodifica brotli.

## Decisión

Las librerías de `assets/` van comprimidas con brotli (`build.rs`). `[profile.*.build-override] opt-level = 3` optimiza el script de compilación: sin ello, brotli tardaba 39 s.
