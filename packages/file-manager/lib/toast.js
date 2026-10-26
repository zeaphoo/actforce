let listeners = [];
let toastIdCounter = 0;

function subscribe(listener) {
  listeners.push(listener);
  return () => { listeners = listeners.filter(l => l !== listener); };
}

function emit(config) {
  listeners.forEach(l => l(config));
}

function generateId() {
  return `toast-${++toastIdCounter}-${Date.now()}`;
}

function show(message, options = {}) {
  const id = generateId();
  emit({
    id,
    message,
    type: options.type || 'info',
    duration: options.duration ?? 5000,
    action: options.action || null,
  });
  return id;
}

function success(message, options = {}) { return show(message, { ...options, type: 'success' }); }
function error(message, options = {}) { return show(message, { ...options, type: 'error' }); }
function info(message, options = {}) { return show(message, { ...options, type: 'info' }); }
function warning(message, options = {}) { return show(message, { ...options, type: 'warning' }); }
function dismiss(id) { emit({ id, dismiss: true }); }

export const toast = { show, success, error, info, warning, dismiss, subscribe };
