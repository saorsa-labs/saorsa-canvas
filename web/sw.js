// Saorsa Canvas Service Worker
// Enables offline support and caching

const CACHE_NAME = 'saorsa-canvas-v1';
const ASSETS = [
    '/',
    '/manifest.json'
];

// Install event - cache assets
self.addEventListener('install', (event) => {
    event.waitUntil(
        caches.open(CACHE_NAME)
            .then((cache) => cache.addAll(ASSETS))
            .then(() => self.skipWaiting())
    );
});

// Activate event - clean old caches
self.addEventListener('activate', (event) => {
    event.waitUntil(
        caches.keys()
            .then((keys) => Promise.all(
                keys.filter((key) => key !== CACHE_NAME)
                    .map((key) => caches.delete(key))
            ))
            .then(() => self.clients.claim())
    );
});

// Fetch event - serve from cache, fall back to network
self.addEventListener('fetch', (event) => {
    // Skip WebSocket requests
    if (event.request.url.includes('/ws')) {
        return;
    }

    event.respondWith(
        caches.match(event.request)
            .then((cached) => {
                if (cached) {
                    return cached;
                }

                return fetch(event.request)
                    .then((response) => {
                        // Cache successful GET requests
                        if (response.ok && event.request.method === 'GET') {
                            const clone = response.clone();
                            caches.open(CACHE_NAME)
                                .then((cache) => cache.put(event.request, clone));
                        }
                        return response;
                    })
                    .catch(() => {
                        // Return offline fallback for navigation
                        if (event.request.mode === 'navigate') {
                            return caches.match('/');
                        }
                        return new Response('Offline', { status: 503 });
                    });
            })
    );
});
