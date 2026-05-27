import Navigation from './Navigation'
import PushStatus from './PushStatus'

// Build metadata injected at build time via Vite define
const APP_VERSION = (import.meta as any).env?.VITE_APP_VERSION || 'dev'
const GIT_COMMIT = (import.meta as any).env?.VITE_GIT_COMMIT || 'unknown'
const GIT_BRANCH = (import.meta as any).env?.VITE_GIT_BRANCH || 'unknown'
const GIT_TAG = (import.meta as any).env?.VITE_GIT_TAG || ''
const BUILD_TIME = (import.meta as any).env?.VITE_BUILD_TIME || ''

interface LayoutProps {
  children: React.ReactNode
}

const Layout: React.FC<LayoutProps> = ({ children }) => {
  return (
    <div className="min-h-screen flex flex-col bg-gray-50 dark:bg-gray-900">
      <Navigation />
      <main className="container mx-auto px-4 py-8 flex-1 w-full">
        {children}
      </main>
      <footer className="border-t border-gray-200 dark:border-gray-700 py-4 text-center text-[10px] sm:text-xs text-gray-500 dark:text-gray-400 flex flex-col sm:flex-row gap-1 sm:gap-2 items-center justify-center">
        <span>SnartNet v{APP_VERSION}{GIT_TAG ? ` (${GIT_TAG})` : ''}</span>
        <span className="hidden sm:inline">•</span>
        <span className="font-mono" title={`Full commit: ${GIT_COMMIT}`}>commit {GIT_COMMIT.slice(0, 7)}</span>
        <span className="hidden sm:inline">•</span>
        <span>branch {GIT_BRANCH}</span>
        {BUILD_TIME && (
          <>
            <span className="hidden sm:inline">•</span>
            <span title={BUILD_TIME}>built {new Date(BUILD_TIME).toLocaleString()}</span>
          </>
        )}
      </footer>
      <PushStatus />
    </div>
  )
}

export default Layout