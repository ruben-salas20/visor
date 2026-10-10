# 0032. Paquetes de GStreamer para vídeo

**Estado:** Aceptada

## Decisión

Vídeo web y MSE dependen de plugins de GStreamer del sistema: necesitan `gst-libav` (AAC, H.264) y `gst-plugins-bad` (`opusparse`). Se recomiendan `gst-plugin-va` (decodificación por GPU) y `gst-plugin-dav1d`. Son requisitos del sistema; visor no los empaqueta.
