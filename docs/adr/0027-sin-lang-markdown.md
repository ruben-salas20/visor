# 0027. Sin @codemirror/lang-markdown

**Estado:** Aceptada

## Decisión

`@codemirror/lang-markdown` arrastra los analizadores de HTML, CSS y JavaScript (150 KB). El lenguaje se crea con `@lezer/markdown` + GFM y `new Language(...)`; ahorra 150 KB. Las fórmulas, los `[[enlaces]]` y el front matter son extensiones propias en `src/page/editor.js`, que se mantienen allí. El código se colorea con highlight.js de `assets/`, no con lenguajes de CodeMirror.
