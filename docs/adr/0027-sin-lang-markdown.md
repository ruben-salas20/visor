# 0027. Sin @codemirror/lang-markdown

**Estado:** Aceptada

## Contexto

`@codemirror/lang-markdown` arrastra los analizadores de HTML, CSS y JavaScript (150 KB).

## Decisión

El lenguaje se crea con `@lezer/markdown` + GFM y `new Language(...)`. Las fórmulas, los `[[enlaces]]` y el front matter son extensiones propias en `src/editor.js`. El código se colorea con highlight.js de `assets/`, no con lenguajes de CodeMirror.

## Consecuencias

Se ahorran 150 KB. Las extensiones propias se mantienen en `src/editor.js`.
