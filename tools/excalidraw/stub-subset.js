// Stands in for Excalidraw's font subsetting (harfbuzz and woff2 as WebAssembly,
// 1.8 MB). It is only used to embed fonts in exported SVGs; without it the whole
// font is embedded, which is what the library itself does when subsetting fails.
const toBase64 = async (buffer) => {
  let text = "";
  const bytes = new Uint8Array(buffer);
  for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return `data:font/woff2;base64,${btoa(text)}`;
};

export const Commands = { Subset: "SUBSET" };
export const subsetToBase64 = (buffer) => toBase64(buffer);
export const subsetToBinary = async (buffer) => buffer;
export { toBase64 };
// Never read: build.mjs turns the library's Worker check off. The import must
// still resolve, so the name stays exported.
export const WorkerUrl = undefined;
