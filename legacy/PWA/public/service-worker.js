self.addEventListener("install", event => {
  event.waitUntil(
    caches.open("snartnet-cache").then(cache => {
      return cache.addAll(["/net/", "/net/index.html"]);
    })
  );
});

self.addEventListener("fetch", event => {
  event.respondWith(
    caches.match(event.request).then(resp => resp || fetch(event.request))
  );
});
