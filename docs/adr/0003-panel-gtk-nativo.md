# 0003. Panel lateral en GTK nativo

**Estado:** Aceptada · **Fecha:** 2026-10-07

## Contexto

El panel era un webview, y cada webview cuesta un proceso web de WebKit.

## Decisión

El panel se dibuja con widgets GTK nativos (`ui/sidebar.rs`).

## Consecuencias

Un proceso de WebKit menos; el arranque y el binario no ganaban nada con el webview.

## Cifras

2026-10-07, 8 rondas alternadas:

- Memoria en reposo con una nota: 313 → 246 MB. Con el editor abierto: 349 → 286 MB.
- Arranque: sin cambio (~271 ms).
- Binario: +72 bytes.
