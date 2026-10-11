// What the drawing editor of visor uses. Mermaid is not here on purpose: build.mjs
// swaps @excalidraw/mermaid-to-excalidraw for stub-mermaid.js (mermaid and katex
// are megabytes). LZString is the lz-string package: Obsidian's Excalidraw
// plugin stores scenes compressed with it.
import React from "react";
import { createRoot } from "react-dom/client";
import LZString from "lz-string";
import {
  Excalidraw,
  serializeAsJSON,
  restore,
  restoreElements,
  exportToSvg,
  convertToExcalidrawElements,
  MIME_TYPES,
} from "@excalidraw/excalidraw";

window.Excalidraw = {
  React,
  createRoot,
  Excalidraw,
  serializeAsJSON,
  restore,
  restoreElements,
  exportToSvg,
  convertToExcalidrawElements,
  MIME_TYPES,
  LZString,
};
