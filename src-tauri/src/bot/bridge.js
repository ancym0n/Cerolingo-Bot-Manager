(() => {
  if (window.__cerolingoSolverInjected) return;
  window.__cerolingoSolverInjected = true;

  const INITIAL_CONFIG = window.__CEROLINGO_CONFIG__ || {};
  const STORAGE_PREFIX = "cerolingo_ext_";
  const memStore = new Map();
  const listeners = [];

  function safeGetItem(key) {
    try {
      const val = window.localStorage.getItem(key);
      if (val !== null) return val;
    } catch (_) {}
    return memStore.has(key) ? memStore.get(key) : null;
  }

  function safeSetItem(key, val) {
    const str = String(val);
    memStore.set(key, str);
    try {
      window.localStorage.setItem(key, str);
    } catch (_) {}
  }

  function readStoreKey(key) {
    const raw = safeGetItem(STORAGE_PREFIX + key);
    if (raw !== null) {
      try {
        return JSON.parse(raw);
      } catch (_) {
        return raw;
      }
    }
    if (key in INITIAL_CONFIG) return INITIAL_CONFIG[key];
    return undefined;
  }

  function writeStoreKey(key, val) {
    const oldVal = readStoreKey(key);
    safeSetItem(STORAGE_PREFIX + key, JSON.stringify(val));
    return { oldValue: oldVal, newValue: val };
  }

  Object.entries(INITIAL_CONFIG).forEach(([k, v]) => {
    if (safeGetItem(STORAGE_PREFIX + k) === null) {
      safeSetItem(STORAGE_PREFIX + k, JSON.stringify(v));
    }
    if (k === "audioBlock" || k === "autoSolver" || k === "botHighlight") {
      safeSetItem(k, String(v));
    }
  });

  const ASSETS = /*__ASSETS__*/ {};

  if (!window.chrome) window.chrome = {};
  if (!window.chrome.runtime) {
    window.chrome.runtime = {
      getURL: (p) => ASSETS[p] || p,
      getManifest: () => ({ version: /*__VERSION__*/ "0" }),
    };
  }
  if (!window.chrome.storage) {
    window.chrome.storage = {
      local: {
        get: (keys, cb) => {
          const result = {};
          const list = Array.isArray(keys)
            ? keys
            : typeof keys === "string"
              ? [keys]
              : keys && typeof keys === "object"
                ? Object.keys(keys)
                : [];
          list.forEach((k) => {
            const val = readStoreKey(k);
            if (val !== undefined) {
              result[k] = val;
            } else if (
              keys &&
              typeof keys === "object" &&
              !Array.isArray(keys) &&
              k in keys
            ) {
              result[k] = keys[k];
            }
          });
          const p = Promise.resolve(result);
          if (typeof cb === "function") p.then(cb);
          return p;
        },
        set: (items, cb) => {
          const changes = {};
          Object.entries(items || {}).forEach(([k, v]) => {
            changes[k] = writeStoreKey(k, v);
          });
          const p = Promise.resolve().then(() => {
            listeners.forEach((fn) => {
              try {
                fn(changes, "local");
              } catch (_) {}
            });
            if (typeof cb === "function") cb();
          });
          return p;
        },
      },
      onChanged: {
        addListener: (fn) => {
          if (typeof fn === "function") listeners.push(fn);
        },
      },
    };
  }

  const injectStyles = () => {
    if (document.getElementById("cerolingo-ext-styles")) return;
    const target = document.head || document.documentElement;
    if (!target) return;
    const style = document.createElement("style");
    style.id = "cerolingo-ext-styles";
    style.textContent = /*__CSS__*/ "";
    target.appendChild(style);
  };

  injectStyles();
  document.addEventListener("DOMContentLoaded", injectStyles, { once: true });
  setInterval(injectStyles, 500);

  /*__MODULES__*/
})();
