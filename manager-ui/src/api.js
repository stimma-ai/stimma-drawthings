// API base is relative to wherever this UI is served from, so it works both
// directly (http://127.0.0.1:8765/stp-v1/manage/) and behind the Stimma app proxy.
const here = new URL('.', location.href)
export const apiBase = new URL('api/', here).toString().replace(/\/$/, '')

async function req(method, path, body, requestOpts = {}) {
  const opts = { method, headers: {}, cache: requestOpts.cache, signal: requestOpts.signal }
  if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json'
    opts.body = JSON.stringify(body)
  }
  const r = await fetch(`${apiBase}${path}`, opts)
  let data = null
  try { data = await r.json() } catch { /* non-JSON */ }
  if (!r.ok) {
    const err = new Error((data && data.error) || `HTTP ${r.status}`)
    err.status = r.status
    err.data = data
    throw err
  }
  return data
}

export const api = {
  overview: (options = {}) => {
    const controller = new AbortController()
    const timer = options.timeout ? setTimeout(() => controller.abort(), options.timeout) : null
    return req('GET', '/overview', undefined, { signal: controller.signal, cache: 'no-store' })
      .finally(() => { if (timer) clearTimeout(timer) })
  },
  tools: () => req('GET', '/tools', undefined, { cache: 'no-store' }),
  toolDetail: (id, file) => req('GET', `/tools/${encodeURIComponent(id)}${file ? `?file=${encodeURIComponent(file)}` : ''}`, undefined, { cache: 'no-store' }),
  setup: (id, file) => req('POST', `/tools/${encodeURIComponent(id)}/setup`, file ? { file } : {}),
  action: (action, file) => req('POST', '/action', file ? { action, file } : { action }),
  clearDone: () => req('POST', '/activity/clear-done'),
}

export function fmtBytes(n) {
  if (n == null) return '?'
  const u = ['B', 'KB', 'MB', 'GB', 'TB']
  let i = 0
  let v = n
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++ }
  return i <= 2 ? `${Math.round(v)} ${u[i]}` : `${v.toFixed(v >= 10 ? 0 : 1)} ${u[i]}`
}

export function fmtAgo(ts) {
  if (!ts) return ''
  const s = Math.max(0, Date.now() / 1000 - ts)
  if (s < 60) return 'just now'
  if (s < 3600) return `${Math.floor(s / 60)} min ago`
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`
  return `${Math.floor(s / 86400)} d ago`
}

export function fmtElapsed(ts) {
  if (!ts) return ''
  const s = Math.max(0, Math.floor(Date.now() / 1000 - ts))
  const m = Math.floor(s / 60)
  return `${m}:${String(s % 60).padStart(2, '0')}`
}
