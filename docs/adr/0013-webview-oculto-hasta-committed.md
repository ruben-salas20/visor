# 0013. Un webview nuevo se crea oculto

**Estado:** Aceptada

**Fecha:** 2026-10-07

## Contexto

Un webview visible desde el principio hacía que WebKitGTK bloqueara el hilo principal hasta 500 ms, esperando el primer pintado. Mientras, el proceso nuevo esperaba al hilo principal para conectarse a la red. Pasaba dos veces (una por DMA-BUF).

## Decisión

Un webview nuevo se crea oculto y se muestra al empezar a cargar (`Committed`).

## Consecuencias

Abrir una pestaña nueva tarda mucho menos (ver «Cifras»).

## Cifras

- Visible desde el principio: abrir una pestaña nueva tardaba ~1,05 s.
- Oculto: 126–179 ms.
- Método (2026-10-07): registros de `WEBKIT_DEBUG=all` en el journal y un temporizador de 16 ms que detectaba los bloqueos.
