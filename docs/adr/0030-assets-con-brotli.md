# 0030. Librerías de assets comprimidas con brotli

**Estado:** Aceptada

## Contexto

WebKit solo anuncia gzip por HTTP, pero descodifica brotli.

## Decisión

Las librerías de `assets/` van comprimidas con brotli. `build-override` optimiza el script de compilación.

## Consecuencias

Sin optimizar, brotli tardaba 39 s.
