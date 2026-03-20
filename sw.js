// Service Worker for The Battlefield PWA
// CACHE_NAME is updated by build.sh with a hash from the wasm binary.
// The browser byte-diffs this file on navigation — a changed CACHE_NAME
// triggers install → precache new assets → activate → delete old caches.
const CACHE_NAME = 'battlefield-15357077';

// Assets to precache on install (app shell)
const PRECACHE_ASSETS = [
  '/index.html',
  '/manifest.json',
  '/pkg/the_battlefield.js',
  '/pkg/the_battlefield_bg.wasm',
  '/icons/icon-192.png',
  '/icons/icon-512.png',
];

// Install: precache shell assets into the new versioned cache
self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(PRECACHE_ASSETS))
  );
  self.skipWaiting();
});

// Activate: delete all caches except the current version
self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(
        keys.filter((key) => key !== CACHE_NAME).map((key) => caches.delete(key))
      )
    ).then(() => self.clients.claim())
  );
});

// Fetch: network-first for app shell, cache-first for game assets
self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);

  // Skip non-GET and cross-origin
  if (event.request.method !== 'GET' || url.origin !== location.origin) {
    return;
  }

  // Game asset files (sprites, textures): cache-first for speed
  if (url.pathname.startsWith('/assets/')) {
    event.respondWith(
      caches.match(event.request).then((cached) => {
        if (cached) return cached;
        return fetch(event.request).then((response) => {
          if (response.ok) {
            const clone = response.clone();
            caches.open(CACHE_NAME).then((cache) => cache.put(event.request, clone));
          }
          return response;
        });
      })
    );
    return;
  }

  // App shell (HTML, JS, WASM): network-first, cache fallback for offline
  event.respondWith(
    fetch(event.request)
      .then((response) => {
        if (response.ok) {
          const clone = response.clone();
          caches.open(CACHE_NAME).then((cache) => cache.put(event.request, clone));
        }
        return response;
      })
      .catch(() => caches.match(event.request))
  );
});
