// The T-13 offline contract: "works offline after first load." The service
// worker precaches every asset the page needs — the page makes no other
// network requests, so cache-first answers everything from here on.
const CACHE = 'lintl-playground-v1';
const ASSETS = [
  './',
  './index.html',
  './styles.css',
  './app.js',
  './sw.js',
  './assets/lintl_wasm.js',
  './assets/lintl_wasm_bg.wasm',
];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(ASSETS))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);
  if (url.origin !== self.location.origin) return;
  event.respondWith(
    caches.match(event.request).then((hit) => hit ?? fetch(event.request)),
  );
});
