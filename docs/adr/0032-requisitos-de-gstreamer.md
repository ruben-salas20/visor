# 0032. Paquetes de GStreamer para vídeo

**Estado:** Aceptada

## Contexto

El vídeo web y MSE dependen de plugins de GStreamer del sistema.

## Decisión

Vídeo web y MSE necesitan `gst-libav` (AAC, H.264) y `gst-plugins-bad` (`opusparse`). Se recomiendan `gst-plugin-va` (decodificación por GPU) y `gst-plugin-dav1d`.

## Consecuencias

Son requisitos del sistema, no se empaquetan con visor.
