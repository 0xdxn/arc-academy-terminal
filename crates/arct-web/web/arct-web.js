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

// Resolves to {module, instance}: the module is kept so a fresh instance
// can replace one that trapped
async function instantiate(source) {
  if (source instanceof ArrayBuffer || ArrayBuffer.isView(source)) {
    return WebAssembly.instantiate(source, {});
  }
  const response = await fetch(source);
  if (!response.ok) {
    throw new Error(`Could not load the lesson engine (${response.status})`);
  }
  if (WebAssembly.instantiateStreaming) {
    try {
      return await WebAssembly.instantiateStreaming(response.clone(), {});
    } catch (_) {
      // Served without application/wasm: fall back to bytes
    }
  }
  return WebAssembly.instantiate(await response.arrayBuffer(), {});
}

/// Load the engine. `source` is the .wasm's URL (next to this file by
/// default) or its bytes.
export async function load(source = new URL("./arct_web.wasm", import.meta.url)) {
  const { module, instance } = await instantiate(source);
  let wasm = instance.exports;
  let lesson = null;

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

  // If the engine traps (it ran out of memory, say), it can't be used
  // again: start a fresh one and the same lesson from the top
  function guarded(run) {
    try {
      return run();
    } catch (err) {
      if (!(err instanceof WebAssembly.RuntimeError)) throw err;
      wasm = new WebAssembly.Instance(module, {}).exports;
      const state = lesson === null ? {} : call(wasm.arct_start, lesson);
      return { ...state, restarted: true, error: "The lesson engine had to restart, so this lesson starts over." };
    }
  }

  return {
    listLessons: () => guarded(() => take(wasm.arct_list_lessons())),
    start: (lessonId) => guarded(() => {
      const state = call(wasm.arct_start, String(lessonId));
      if (!state.error) lesson = String(lessonId);
      return state;
    }),
    submit: (line) => guarded(() => call(wasm.arct_submit, String(line))),
    reset: () => guarded(() => take(wasm.arct_reset())),
    state: () => guarded(() => take(wasm.arct_state())),
  };
}
