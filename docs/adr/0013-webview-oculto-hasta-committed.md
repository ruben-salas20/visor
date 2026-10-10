# 0013. Un webview nuevo se crea oculto

**Estado:** Aceptada · **Fecha:** 2026-10-07

## Contexto

Con el webview visible desde el principio, WebKitGTK bloqueaba el hilo principal hasta 500 ms esperando el primer pintado, y el proceso nuevo esperaba a ese hilo para conectarse a la red. Pasaba dos veces (una por DMA-BUF).

## Decisión

Un webview nuevo se crea oculto y se muestra al empezar a cargar (`Committed`).

## Cifras

Abrir una pestaña nueva: ~1,05 s visible desde el principio, 126–179 ms oculto. Método (2026-10-07): registros de `WEBKIT_DEBUG=all` en el journal y un temporizador de 16 ms que detectaba los bloqueos.
