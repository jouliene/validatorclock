// The page is one script: every file under public/app, public/shared and public/app.js is
// concatenated in the order of APP_JS_PARTS (src/server/assets/embedded.rs) into one global
// scope. A test therefore loads the files it needs into its own context, in that order, and
// calls the functions directly - there is nothing to import and nothing to build.
//
// Two stubs are all the browser these files need before a function is called: the address
// and source-display preferences read localStorage as the bundle evaluates, and state.js
// reads the script tag it was served with to version asset URLs. A test that reaches into
// the DOM needs more than this, and belongs in a browser rather than here.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { runInThisContext } from "node:vm";

const PUBLIC_DIR = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "public");

export function stubBrowser({ localStorage = {} } = {}) {
  globalThis.window = {
    localStorage: {
      getItem: (key) => (key in localStorage ? localStorage[key] : null),
      setItem: (key, value) => {
        localStorage[key] = String(value);
      },
    },
  };
  globalThis.document = { currentScript: null, getElementById: () => null };
  return localStorage;
}

/// Load bundle files into this context, in the order the browser would.
export function load(...files) {
  for (const file of files) {
    runInThisContext(readFileSync(join(PUBLIC_DIR, file), "utf8"), { filename: file });
  }
}
