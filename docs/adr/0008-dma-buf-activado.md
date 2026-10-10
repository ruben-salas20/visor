# 0008. DMA-BUF activado

**Estado:** Aceptada

## Contexto

Desactivar DMA-BUF adelanta el arranque, pero perjudica el dibujo con Three.js, que el usuario usa mucho.

## Decisión

DMA-BUF queda activado. Es el valor por defecto de WebKitGTK: el código no lo toca, y no debe tocarlo.

## Cifras

Sin DMA-BUF el arranque es ~0,5 s más rápido, pero Three.js cae de 60 a 26 fps.
