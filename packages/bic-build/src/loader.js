// Registers the BIC Service Worker. It finds its own location from its script
// URL, so it works under any base path. The page does not wait for it: images
// load as PNG until the worker controls the page.
(function () {
  if (!('serviceWorker' in navigator)) return;
  var script = document.currentScript;
  if (!script) return;
  var worker = new URL('bic-sw.js', script.src);
  var scope = new URL('./', script.src);
  navigator.serviceWorker.register(worker.href, { scope: scope.pathname }).catch(function () {});
})();
