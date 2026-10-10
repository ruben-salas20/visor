// What src/page/editor.js uses. Markdown is parsed with @lezer/markdown
// directly: @codemirror/lang-markdown would pull in the HTML, CSS and
// JavaScript parsers (about 150 KB).
export { EditorState, EditorSelection, StateField, StateEffect, Prec } from "@codemirror/state";
export { EditorView, ViewPlugin, Decoration, WidgetType, keymap, drawSelection } from "@codemirror/view";
export { history, defaultKeymap, historyKeymap, insertTab } from "@codemirror/commands";
export { Language, defineLanguageFacet, languageDataProp, syntaxTree, syntaxHighlighting, HighlightStyle } from "@codemirror/language";
export { parser, GFM } from "@lezer/markdown";
export { tags, styleTags } from "@lezer/highlight";
