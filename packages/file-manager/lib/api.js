// API helpers for file-manager
const BASE = '';

export function getToken() {
  // actforce web stores the JWT under 'actforce-token'; fall back to the
  // legacy 'token' key so file-manager requests carry the Authorization header.
  return localStorage.getItem('actforce-token') || localStorage.getItem('token') || '';
}

export function authHeaders() {
  const token = getToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}

export async function apiFetch(url, options = {}) {
  const headers = {
    ...authHeaders(),
    ...options.headers,
  };
  const res = await fetch(url, { ...options, headers });
  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new Error(`HTTP ${res.status}: ${text || res.statusText}`);
  }
  return res;
}

export async function apiJson(url, options = {}) {
  const res = await apiFetch(url, options);
  return res.json();
}
