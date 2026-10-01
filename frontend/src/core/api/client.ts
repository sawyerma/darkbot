const API_BASE = (import.meta.env.VITE_API_BASE_URL || "/api/v1").replace(/\/$/, "");

export async function apiGet<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path.startsWith("/") ? path : `/${path}`}`, {
    ...init,
    headers: { Accept: "application/json", ...(init?.headers || {}) },
  });
  if (!response.ok) throw new Error(`HTTP ${response.status} ${response.statusText}`);
  return response.json() as Promise<T>;
}
