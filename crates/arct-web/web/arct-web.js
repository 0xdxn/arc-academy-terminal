// arct-web.js: loads arct_web.wasm (the Arc Academy Terminal lesson
// engine) and wraps its string API. No dependencies, no network beyond
// fetching the .wasm itself, nothing stored.
//
//   import { load } from "./arct-web.js";
//   const arct = await load();                  // or load(bytes) / load(url)
//   arct.listLessons();                         // [{id, title, track, difficulty}]
//   let state = arct.start("explorer-look-around");
//   state = arct.submit("ls");                  // {entries, step, done, ...}
//   state = arct.reset();
//
// Every call returns parsed JSON; errors come back as {error}.

const encoder = new TextEncoder();
const decoder = new TextDecoder();

async function instantiate(source) {
  if (source instanceof ArrayBuffer || ArrayBuffer.isView(source)) {
    return (await WebAssembly.instantiate(source, {})).instance;
  }
  const response = await fetch(source);
  if (!response.ok) {
    throw new Error(`Could not load the lesson engine (${response.status})`);
  }
  if (WebAssembly.instantiateStreaming) {
    try {
      return (await WebAssembly.instantiateStreaming(response.clone(), {})).instance;
    } catch (_) {
      // Served without application/wasm: fall back to bytes
    }
  }
  return (await WebAssembly.instantiate(await response.arrayBuffer(), {})).instance;
}

/// Load the engine. `source` is the .wasm's URL (next to this file by
/// default) or its bytes.
export async function load(source = new URL("./arct_web.wasm", import.meta.url)) {
  const instance = await instantiate(source);
  const wasm = instance.exports;

  // A result is a 4-byte little-endian length, then that much UTF-8 JSON
  function take(ptr) {
    const len = new DataView(wasm.memory.buffer).getUint32(ptr, true);
    const text = decoder.decode(new Uint8Array(wasm.memory.buffer, ptr + 4, len));
    wasm.arct_free(ptr, len + 4);
    return JSON.parse(text);
  }

  // Pass a string in: written into a buffer the engine reserves
  function call(fn, text) {
    const bytes = encoder.encode(text);
    const ptr = wasm.arct_alloc(bytes.length);
    new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
    try {
      return take(fn(ptr, bytes.length));
    } finally {
      wasm.arct_free(ptr, bytes.length);
    }
  }

  return {
    listLessons: () => take(wasm.arct_list_lessons()),
    start: (lessonId) => call(wasm.arct_start, String(lessonId)),
    submit: (line) => call(wasm.arct_submit, String(line)),
    reset: () => take(wasm.arct_reset()),
    state: () => take(wasm.arct_state()),
  };
}
