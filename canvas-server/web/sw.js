// Saorsa Canvas Service Worker
// Enables offline support, caching, and background sync

const CACHE_NAME = 'saorsa-canvas-v3';
const OFFLINE_QUEUE_STORE = 'offline-queue';
const DB_NAME = 'saorsa-canvas-db';
const DB_VERSION = 1;

const ASSETS = [
    '/',
    '/manifest.json',
    '/pkg/canvas_app.js',
    '/pkg/canvas_app_bg.wasm',
    '/looking-glass.js',
    '/video.js'
];

// IndexedDB helpers for offline queue persistence
function openDB() {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open(DB_NAME, DB_VERSION);

        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve(request.result);

        request.onupgradeneeded = (event) => {
            const db = event.target.result;

            // Create offline queue store
            if (!db.objectStoreNames.contains(OFFLINE_QUEUE_STORE)) {
                const store = db.createObjectStore(OFFLINE_QUEUE_STORE, {
                    keyPath: 'id',
                    autoIncrement: true
                });
                store.createIndex('timestamp', 'timestamp', { unique: false });
            }
        };
    });
}

async function saveToQueue(operation) {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const tx = db.transaction(OFFLINE_QUEUE_STORE, 'readwrite');
        const store = tx.objectStore(OFFLINE_QUEUE_STORE);

        const request = store.add({
            ...operation,
            timestamp: Date.now()
        });

        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
}

async function getQueuedOperations() {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const tx = db.transaction(OFFLINE_QUEUE_STORE, 'readonly');
        const store = tx.objectStore(OFFLINE_QUEUE_STORE);
        const index = store.index('timestamp');

        const request = index.getAll();
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
}

async function clearQueue() {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const tx = db.transaction(OFFLINE_QUEUE_STORE, 'readwrite');
        const store = tx.objectStore(OFFLINE_QUEUE_STORE);

        const request = store.clear();
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
    });
}

async function removeFromQueue(id) {
    const db = await openDB();
    return new Promise((resolve, reject) => {
        const tx = db.transaction(OFFLINE_QUEUE_STORE, 'readwrite');
        const store = tx.objectStore(OFFLINE_QUEUE_STORE);

        const request = store.delete(id);
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
    });
}

// Install event - cache assets
self.addEventListener('install', (event) => {
    event.waitUntil(
        caches.open(CACHE_NAME)
            .then((cache) => {
                // Try to cache all assets, but don't fail if some are missing
                return Promise.allSettled(
                    ASSETS.map(url =>
                        cache.add(url).catch(err => {
                            console.warn('Failed to cache:', url, err);
                            return null;
                        })
                    )
                );
            })
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

// Fetch event - serve from cache, fall back to network, queue POST/PUT if offline
self.addEventListener('fetch', (event) => {
    const url = new URL(event.request.url);

    // Skip WebSocket requests
    if (url.pathname === '/ws') {
        return;
    }

    // Handle MCP/API requests specially
    if (url.pathname === '/mcp' || url.pathname.startsWith('/api/')) {
        event.respondWith(handleApiRequest(event.request));
        return;
    }

    // Standard cache-first for assets
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

// Handle API requests with offline queuing
async function handleApiRequest(request) {
    try {
        // Try network first for API requests
        const response = await fetch(request.clone());
        return response;
    } catch (error) {
        // Network failed - queue the request if it's a mutation
        if (request.method === 'POST' || request.method === 'PUT' || request.method === 'DELETE') {
            try {
                const body = await request.clone().json();
                await saveToQueue({
                    url: request.url,
                    method: request.method,
                    body: body,
                    headers: Object.fromEntries(request.headers.entries())
                });

                // Register for background sync
                if ('sync' in self.registration) {
                    await self.registration.sync.register('canvas-sync');
                }

                // Return success response indicating queued
                return new Response(JSON.stringify({
                    queued: true,
                    message: 'Operation queued for sync when online'
                }), {
                    status: 202,
                    headers: { 'Content-Type': 'application/json' }
                });
            } catch (queueError) {
                console.error('Failed to queue operation:', queueError);
            }
        }

        // Return offline error
        return new Response(JSON.stringify({
            error: 'offline',
            message: 'No network connection'
        }), {
            status: 503,
            headers: { 'Content-Type': 'application/json' }
        });
    }
}

// Background sync event - replay queued operations
self.addEventListener('sync', (event) => {
    if (event.tag === 'canvas-sync') {
        event.waitUntil(syncQueuedOperations());
    }
});

async function syncQueuedOperations() {
    const operations = await getQueuedOperations();

    if (operations.length === 0) {
        return;
    }

    console.log(`Syncing ${operations.length} queued operations`);

    const results = {
        synced: 0,
        failed: 0
    };

    for (const op of operations) {
        try {
            const response = await fetch(op.url, {
                method: op.method,
                headers: op.headers,
                body: JSON.stringify(op.body)
            });

            if (response.ok) {
                await removeFromQueue(op.id);
                results.synced++;
            } else {
                // Server rejected - might be a conflict
                console.warn('Sync rejected:', response.status, await response.text());
                results.failed++;
            }
        } catch (error) {
            // Still offline or network error
            console.error('Sync failed:', error);
            results.failed++;
            // Stop trying if network is down
            break;
        }
    }

    // Notify clients of sync result
    const clients = await self.clients.matchAll();
    for (const client of clients) {
        client.postMessage({
            type: 'sync-complete',
            results
        });
    }

    return results;
}

// Listen for messages from the main thread
self.addEventListener('message', (event) => {
    const { type, data } = event.data;

    switch (type) {
        case 'queue-operation':
            saveToQueue(data)
                .then(() => {
                    event.source.postMessage({
                        type: 'queue-success',
                        id: data.id
                    });
                })
                .catch((error) => {
                    event.source.postMessage({
                        type: 'queue-error',
                        error: error.message
                    });
                });
            break;

        case 'get-queue-status':
            getQueuedOperations()
                .then((ops) => {
                    event.source.postMessage({
                        type: 'queue-status',
                        count: ops.length,
                        operations: ops
                    });
                });
            break;

        case 'force-sync':
            syncQueuedOperations()
                .then((results) => {
                    event.source.postMessage({
                        type: 'force-sync-complete',
                        results
                    });
                });
            break;

        case 'clear-queue':
            clearQueue()
                .then(() => {
                    event.source.postMessage({
                        type: 'queue-cleared'
                    });
                });
            break;
    }
});

// Periodic sync for browsers that support it
self.addEventListener('periodicsync', (event) => {
    if (event.tag === 'canvas-periodic-sync') {
        event.waitUntil(syncQueuedOperations());
    }
});
