# visor

[English](README.md)

visor es una aplicación de escritorio para estudiar con inteligencia artificial (IA). Muestra una **bóveda**, que es una carpeta con páginas HTML y notas Markdown. Tú lees, subrayas y comentas. Tu agente de IA lee tus anotaciones y te responde en la misma página.

Funciona solo en Linux con Wayland.

## Qué necesitas

- Linux con Wayland, GTK 3 y WebKitGTK 4.1 (versión 2.38 o posterior).
- Rust 1.88 o posterior, para compilar.
- Para ver vídeo de la web: los paquetes `gst-libav` y `gst-plugins-bad`. Se recomiendan también `gst-plugin-va` (decodifica con la tarjeta gráfica) y `gst-plugin-dav1d`.

## Instalar

1. Compila el programa:

   ```
   cargo build --release
   ```

2. Copia `target/release/visor` a una carpeta de tu `PATH`, por ejemplo `~/.local/bin`.

## Abrir una bóveda

- Desde la aplicación: abre `visor` y pulsa Ctrl+O para elegir una carpeta.
- Desde la terminal: `visor open <carpeta>`.

Para ver un solo archivo HTML sin crear una bóveda, usa `visor open <archivo.html>`. Ese archivo no admite anotaciones.

visor recuerda las pestañas de cada bóveda y la reabre como la dejaste. Si un archivo cambia en el disco, la página se actualiza sola.

## Atajos principales

| Atajo | Acción |
|---|---|
| Ctrl+O | Abrir una carpeta |
| Ctrl+B | Mostrar u ocultar el panel (el árbol de la bóveda) |
| Ctrl+T, Ctrl+W | Abrir o cerrar una pestaña |
| Ctrl+\| o Ctrl+\ | Dividir la ventana en dos lados, o volver a uno |
| Ctrl+E | Editar la nota abierta, o volver a leerla |
| Ctrl+N | Crear una nota nueva |
| Ctrl+Shift+A | Anotar un elemento o un punto de la página |
| Ctrl+Shift+L | Ver la lista de anotaciones de la página |
| Ctrl+L | Escribir una dirección web o una ruta |

`visor help` lista todos los atajos.

## Anotar y recibir respuestas

1. Selecciona un texto de una página de la bóveda. Elige un color para resaltarlo, o pulsa «Comentar» para escribir tu comentario.
2. Para anotar un gráfico o una imagen, pulsa Ctrl+Shift+A y haz clic en el elemento.
3. Tu agente lee la anotación y responde. Ves la respuesta al momento en tu nota y puedes contestar.

visor guarda las anotaciones en `<bóveda>/.visor/anotaciones/`. No modifican tus páginas. Si tu agente cambia el texto anotado, la anotación se recoloca sola. Si lo borra, queda huérfana y la ves en la lista de anotaciones.

## Conectar tu agente

Tu agente controla visor por la terminal o por MCP, el protocolo con el que los agentes usan herramientas. Para registrarlo en Claude Code:

```
claude mcp add visor -- visor mcp
```

Después pide a tu agente que llame a la herramienta `guide`. Le explica qué órdenes tiene y cómo escribir páginas para visor.

## Problemas frecuentes

- **Un vídeo de la web no se reproduce.** Instala los paquetes de GStreamer de la sección «Qué necesitas».
- **Una página pierde sus datos guardados.** visor usa siempre el mismo puerto local. Si otro programa lo ocupa, visor elige otro y las páginas empiezan sin sus datos esa sesión. Cierra el programa que ocupa el puerto.
- **Una orden de prueba abre páginas en tu ventana.** La terminal habla con la instancia de visor que tienes abierta. Para probar sin tocarla, sigue `docs/pruebas.md`.

## Más documentación

Esta documentación está en español.

- [CONTEXT.md](CONTEXT.md): glosario de términos.
- [docs/decisiones.md](docs/decisiones.md): decisiones de diseño y sus mediciones.
- [docs/pruebas.md](docs/pruebas.md): cómo probar y medir.
- [AGENTS.md](AGENTS.md): guía para agentes que programan en este repositorio.

## Licencia

MIT. Consulta [LICENSE](LICENSE).
