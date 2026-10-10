# 0035. Actualización automática desde GitHub Releases

**Estado:** Aceptada · **Fecha:** 2026-10-10

## Decisión

visor busca en las releases de `ruben-salas20/visor` como mucho una vez al día, en segundo plano y después de arrancar. Si hay una versión nueva, descarga el asset `visor-x86_64-linux`, comprueba su SHA-256 con `sha256sum` (el que publica GitHub) y lo pone en su sitio con un `rename` atómico. La versión nueva se usa al volver a abrir. Los binarios dentro de `target/` no se tocan, y `tools/publicar.sh` sube el asset.

Se usan `curl` y `sha256sum` en lugar de un crate HTTPS: un cliente TLS añadiría 1–2 MB a un binario de 4,2 MB.

Se desactiva en Ajustes (`auto_update: false` en la configuración).
