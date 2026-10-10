# 0021. Vigilancia de archivos

**Estado:** Aceptada

## Contexto

`notify` también avisa al abrir un archivo. Atender ese aviso recargaría la página sin fin, porque servirla la abre.

## Decisión

Los eventos de apertura se descartan. Las escrituras propias (`OwnWrite`, `ui/watch.rs`) se reconocen por el hash del contenido escrito, no por una ventana de tiempo, para no tapar los cambios de un agente.

Los cambios se agrupan en lotes: el lote sale tras 150 ms sin cambios, o como mucho 1 s después del primero y con un máximo de 5000 rutas distintas, aunque sigan llegando cambios. Sin ese tope, un proceso que escribe sin parar en la bóveda impediría que el lote llegara nunca. El resto de cambios va en el lote siguiente.
