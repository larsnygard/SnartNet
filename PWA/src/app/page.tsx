import React from 'react'

const HomePage: React.FC = () => {
  return (
    <div>
      <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-4">
        SnartNet
      </h1>
      <p className="text-lg text-gray-600 dark:text-gray-300 mb-8">
        Decentralized social media, powered by swarms.
      </p>
      
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
        <h2 className="text-xl font-semibold mb-4">Getting Started</h2>
        <p className="text-gray-600 dark:text-gray-300 mb-4">
          Welcome to SnartNet! This is a decentralized social media platform built on 
          BitTorrent-like swarm technology and modern cryptography.
        </p>
        <div className="space-y-2">
          <p className="text-sm text-gray-500">
            🔐 Your identity is cryptographically secured
          </p>
          <p className="text-sm text-gray-500">
            🌐 Content is distributed peer-to-peer
          </p>
          <p className="text-sm text-gray-500">
            🤝 Trust is managed through your Ring of Trust
          </p>
        </div>
      </div>
    </div>
  )
}

export default HomePage
