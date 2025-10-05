// Lightweight opt-in debug/perf helpers. Activated when localStorage key
//  snartnet:debug:perf === '1'
// Usage: import { perfMark, perfMeasure, startMemoryLogging } from '@/lib/debug'
// Call startMemoryLogging() once to log periodic memory stats (Chrome only).

const enabled = (() => {
  try { return localStorage.getItem('snartnet:debug:perf') === '1' } catch { return false }
})()

export function perfMark(name: string) {
  if (!enabled || typeof performance === 'undefined' || !performance.mark) return
  try { performance.mark(name) } catch {}
}

export function perfMeasure(name: string, start: string, end?: string) {
  if (!enabled || typeof performance === 'undefined' || !performance.measure) return
  try {
    const measureName = `${name}`
    performance.measure(measureName, start, end)
    const entries = performance.getEntriesByName(measureName)
    const last = entries[entries.length - 1]
    if (last) console.info(`[perf] ${measureName}: ${last.duration.toFixed(2)}ms`)
  } catch {}
}

let memoryLoggerStarted = false
export function startMemoryLogging(intervalMs = 5000) {
  if (!enabled || memoryLoggerStarted) return
  memoryLoggerStarted = true
  if (!(performance as any).memory) {
    console.info('[perf] performance.memory not available in this browser')
    return
  }
  const log = () => {
    const m: any = (performance as any).memory
    if (m) {
      const used = (m.usedJSHeapSize / 1024 / 1024).toFixed(1)
      const total = (m.totalJSHeapSize / 1024 / 1024).toFixed(1)
      const limit = (m.jsHeapSizeLimit / 1024 / 1024).toFixed(1)
      console.info(`[mem] used=${used}MB total=${total}MB limit=${limit}MB`)    
    }
    setTimeout(log, intervalMs)
  }
  setTimeout(log, intervalMs)
}

export const debugEnabled = enabled
