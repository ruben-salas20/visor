# 0003. Panel lateral en GTK nativo

**Estado:** Aceptada

**Fecha:** 2026-10-07

## Contexto

El panel lateral era un webview. Cada webview cuesta un proceso web de WebKit propio.

## Decisión

El panel se dibuja con widgets GTK nativos (`sidebar.rs`).

## Consecuencias

Se ahorra un proceso de WebKit. El arranque y el tamaño del binario no cambian.

## Antes

El panel era un webview con su propio proceso de WebKit. Se cambió porque ese proceso costaba memoria (ver «Cifras») sin que el arranque ni el binario ganaran nada con él.

## Cifras

- Memoria en reposo con una nota: 313 → 246 MB. Con el editor abierto: 349 → 286 MB (2026-10-07, 8 rondas alternadas).
- Arranque: sin cambio (~271 ms).
- Binario: +72 bytes.
