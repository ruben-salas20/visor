# 0021. Vigilancia de archivos

**Estado:** Aceptada

## Contexto

`notify` también avisa al abrir un archivo. Si se atendiera ese aviso, servir una página la recargaría sin fin.

## Decisión

Los eventos de apertura se descartan. Las escrituras propias se reconocen por el hash del contenido escrito, no por una ventana de tiempo.

## Consecuencias

No se tapan los cambios de un agente.
