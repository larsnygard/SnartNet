import React, { useEffect, useState } from 'react'

interface Stats {
  transport: string
  peers: number
  received: number
  published: number
}

export const PushStatus: React.FC = () => {
  const [stats, setStats] = useState<Stats>({ transport: 'initializing', peers: 0, received: 0, published: 0 })
  const [expanded, setExpanded] = useState(false)

  useEffect(() => {
    const interval = setInterval(() => {
      const w: any = window as any
      const node: any = w.snartnetLibp2p
      const received = (w.__sn_head_sig_cache?.length) || 0
      const published = (w.lastPublishedHeadUpdate ? (w.__sn_head_published_count || 0) : (w.__sn_head_published_count || 0))
      setStats({
        transport: node ? 'libp2p' : 'in-memory',
        peers: node?.getPeers ? node.getPeers().length : (node?.getConnections ? node.getConnections().length : 0),
        received,
        published
      })
    }, 2000)
    return () => clearInterval(interval)
  }, [])

  return (
    <div className="fixed bottom-1 right-1 text-[11px] bg-black/70 text-white px-2 py-1 rounded shadow z-50 select-none">
      <div className="flex gap-2 items-center cursor-pointer" onClick={()=> setExpanded(e=>!e)}>
        <span>Push: {stats.transport}</span>
        <span>| Peers: {stats.peers}</span>
        <span>| Rx: {stats.received}</span>
        <span>| Tx: {stats.published}</span>
        <span>{expanded ? '▾' : '▸'}</span>
      </div>
      {expanded && (
        <div className="mt-1 max-w-[260px] leading-tight space-y-0.5">
          <div>Transport: {stats.transport}</div>
          <div>Peers: {stats.peers}</div>
          <div>Head Updates Received (unique sigs): {stats.received}</div>
          <div>Head Updates Published: {stats.published}</div>
          <div className="opacity-60">Click to collapse</div>
        </div>
      )}
    </div>
  )
}

export default PushStatus
