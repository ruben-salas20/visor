# 0008. DMA-BUF activado

**Estado:** Aceptada

## Contexto

Desactivar DMA-BUF adelanta el arranque, pero perjudica el dibujo con Three.js.

## Decisión

DMA-BUF queda activado.

## Consecuencias

El arranque tarda ~0,5 s más que sin DMA-BUF. El usuario usa mucho Three.js.

## Cifras

- Sin DMA-BUF el arranque es ~0,5 s más rápido, pero Three.js cae de 60 a 26 fps.
