
(input => {
  const markNative = input.markNative;
  const nativeSourceOf = input.nativeSource;
  const originalFunctionToString = Function.prototype.toString;
  const nativeFunction = (name, implementation) => {
    Object.defineProperty(implementation, 'name', { configurable: true, value: name });
    markNative(implementation, 'function ' + name + '() {\n    [native code]\n}');
    return implementation;
  };
  const functionToString = function toString() {
    'use strict';
    const marked = nativeSourceOf(this);
    if (marked !== undefined) return marked;
    return originalFunctionToString.call(this);
  };
  markNative(functionToString, 'function toString() {\n    [native code]\n}');
  Function.prototype.toString = functionToString;
  const timers = input.timers;
  globalThis.setTimeout = nativeFunction('setTimeout', function setTimeout(callback, delay, ...args) { return timers.setTimeout(callback, delay, ...args); });
  globalThis.setInterval = nativeFunction('setInterval', function setInterval(callback, delay, ...args) { return timers.setInterval(callback, delay, ...args); });
  globalThis.clearTimeout = nativeFunction('clearTimeout', function clearTimeout(id) { timers.clear(id); });
  globalThis.clearInterval = nativeFunction('clearInterval', function clearInterval(id) { timers.clear(id); });
  let domAPI;
  let idleCallbackCount = 0;
  let idleHandle = 0;
  const idleCallbacks = new Set();
  globalThis.requestIdleCallback = nativeFunction('requestIdleCallback', function requestIdleCallback(callback, options = {}) {
    const timeout = options.timeout > 0 ? Math.min(options.timeout, 50) : 0;
    const requested = input.preciseNow();
    const timedOut = timeout > 0 && idleCallbackCount++ % 2 === 1;
    const handle = ++idleHandle;
    idleCallbacks.add(handle);
    input.queueTask(timedOut ? timeout : 1, () => {
      if (!idleCallbacks.delete(handle)) return;
      const deadline = timedOut ? 0 : timeout > 0 ? requested + timeout : input.preciseNow() + 50;
      callback(domAPI.idleDeadline(timedOut, () => Math.max(0, Math.min(50, deadline - input.performanceNow()))));
    });
    return handle;
  });
  globalThis.cancelIdleCallback = nativeFunction('cancelIdleCallback', function cancelIdleCallback(handle) { idleCallbacks.delete(handle); });
  domAPI = input.installDOM(input.shape, input.profile, {
    markNative,
    brands: input.brands,
    interfaceParents: input.interfaceParents,
    isA: input.isA,
    brandCheck: input.brandCheck,
    exposed: input.exposed,
    storage: input.storage,
    realm: input.realm,
    topRealm: input.topRealm,
    trust: input.trust,
    setTimeout: globalThis.setTimeout,
    clearTimeout: globalThis.clearTimeout,
    queueTask: input.queueTask,
    incumbent: input.incumbent,
    kindOf: input.kindOf,
    transferBuffer: input.transferBuffer,
    bufferDetached: input.bufferDetached,
    throwLocation: input.throwLocation,
    callListener: input.callListener,
    digest: input.digest,
    encodingName: input.encodingName,
    decodeText: input.decodeText,
    parseURL: input.parseURL,
    updateURL: input.updateURL,
    randomUUID: input.randomUUID,
    randomBytes: input.randomBytes,
    templatedObject: input.templatedObject,
    setTemplateResolver: input.setTemplateResolver,
    performanceNow: input.performanceNow,
    timeOrigin: input.timeOrigin,
    createIframeWindow: input.createIframeWindow,
    loadImage: input.loadImage,
    resolveInterface: input.resolveInterface,
    orderKeys: input.orderKeys,
  });
  return domAPI;
})
