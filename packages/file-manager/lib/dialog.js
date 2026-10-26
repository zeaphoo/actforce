let listeners = [];

function subscribe(listener) {
  listeners.push(listener);
  return () => { listeners = listeners.filter(l => l !== listener); };
}

function emit(config) {
  listeners.forEach(l => l(config));
}

function confirm(message, options = {}) {
  return new Promise((resolve) => {
    emit({ type: 'confirm', message, ...options, onResult: resolve });
  });
}

function prompt(message, options = {}) {
  return new Promise((resolve) => {
    emit({ type: 'prompt', message, ...options, onResult: resolve });
  });
}

function alert(message, options = {}) {
  return new Promise((resolve) => {
    emit({ type: 'alert', message, ...options, onResult: resolve });
  });
}

export const dialog = { confirm, prompt, alert, subscribe };
