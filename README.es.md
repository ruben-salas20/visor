# visor

[English](README.md)

visor es una aplicación para estudiar con inteligencia artificial (IA). Muestra una **bóveda**: una carpeta con páginas HTML y notas Markdown. Tú lees, subrayas y comentas. Tu agente de IA lee tus anotaciones y te responde en la misma página.

visor funciona solo en Linux con Wayland.

## Qué necesitas

- Linux con Wayland, GTK 3 y WebKitGTK 4.1 (versión 2.38 o posterior).
- Rust 1.89 o posterior. Sirve para compilar visor; no hay versión ya compilada.
- Un agente de IA que sepa usar herramientas, como Claude Code.
- Para ver vídeo de la web, estos paquetes de tu distribución: `gst-libav` y `gst-plugins-bad`. También conviene instalar `gst-plugin-va` (usa la tarjeta gráfica) y `gst-plugin-dav1d`.

## Instalar

Escribe estas órdenes en una terminal, dentro de la carpeta de visor.

1. Compila el programa:

   ```
   cargo build --release
   ```

2. Copia el archivo `target/release/visor` a una carpeta donde tu sistema busque programas, por ejemplo `~/.local/bin`.

## Abrir una bóveda

- Desde la aplicación: escribe `visor` en la terminal y pulsa Ctrl+O para elegir una carpeta.
- Desde la terminal: `visor open <carpeta>`, con la ruta de tu carpeta en lugar de `<carpeta>`.

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

visor guarda las anotaciones en `<bóveda>/.visor/anotaciones/`. No modifican tus páginas. Si el texto anotado cambia de sitio en la página, la anotación lo sigue. Si el texto cambia o desaparece, la anotación queda huérfana y la ves en la lista de anotaciones.

## Conectar tu agente

Tu agente controla visor con órdenes de terminal o por MCP, un protocolo para que los agentes usen programas externos. Para registrar visor en Claude Code, escribe:

```
claude mcp add visor -- visor mcp
```

Después pide a tu agente que use la herramienta `guide`. Esa herramienta le explica qué puede hacer con visor y cómo escribir páginas para visor.

## Problemas frecuentes

- **Un vídeo de la web no se reproduce.** Instala los paquetes `gst-libav` y `gst-plugins-bad` que indica la sección «Qué necesitas».
- **Una página pierde sus datos guardados.** visor usa siempre la misma conexión interna de tu equipo (el puerto). Si otro programa la ocupa, visor elige otra y las páginas empiezan sin sus datos esa sesión. Cierra el otro programa y reinicia visor.

## Más documentación

Esta documentación está en español.

- [CONTEXT.md](CONTEXT.md): glosario de términos.
- [docs/adr/](docs/adr/): decisiones de diseño y sus mediciones.
- [docs/pruebas.md](docs/pruebas.md): cómo probar y medir.
- [AGENTS.md](AGENTS.md): guía para agentes que programan en este repositorio.

## Licencia

MIT. Consulta [LICENSE](LICENSE).
