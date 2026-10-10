# 0021. Vigilancia de archivos

**Estado:** Aceptada

## Contexto

`notify` también avisa al abrir un archivo. Atender ese aviso recargaría la página sin fin, porque servirla la abre.

## Decisión

Los eventos de apertura se descartan. Las escrituras propias (`OwnWrite`, `ui/watch.rs`) se reconocen por el hash del contenido escrito, no por una ventana de tiempo, para no tapar los cambios de un agente.
