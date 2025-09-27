import { Link } from 'react-router-dom'

const Navigation: React.FC = () => {
  return (
    <div className="sticky top-0 z-40 backdrop-blur supports-[backdrop-filter]:bg-white/75 dark:supports-[backdrop-filter]:bg-gray-900/75 bg-white/90 dark:bg-gray-800/90 border-b border-gray-200 dark:border-gray-700 shadow-sm">
      <nav>
        <div className="container mx-auto px-4">
          <div className="flex justify-between items-center h-16">
            <Link to="/" className="text-xl font-bold text-gray-900 dark:text-white">
              SnartNet
            </Link>
            <div className="flex space-x-4 items-center">
              <Link to="/" className="text-sm text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-white">Home</Link>
              <Link to="/profile" className="text-sm text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-white">Profile</Link>
              <Link to="/network" className="text-sm text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-white">Network</Link>
              <Link to="/messages" className="text-sm text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-white">Messages</Link>
            </div>
          </div>
        </div>
        <div className="bg-red-600/90 text-white text-center text-xs py-1 tracking-wide font-medium">
          Development version – expect things to break & data to reset
        </div>
      </nav>
    </div>
  )
}

export default Navigation