// Stands in for @excalidraw/mermaid-to-excalidraw, which build.mjs leaves out.
// Only the Mermaid paths of Excalidraw reach this; everything else works.
export async function parseMermaidToExcalidraw() {
  throw new Error(
    "Los diagramas Mermaid no están incluidos en visor: el paquete de Excalidraw se construye sin ellos (tools/excalidraw/README.md).",
  );
}
