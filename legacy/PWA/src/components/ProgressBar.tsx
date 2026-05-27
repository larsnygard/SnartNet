interface ProgressBarProps {
  progress: number
  className?: string
}

export default function ProgressBar({ progress, className = "" }: ProgressBarProps) {
  const progressPercentage = Math.max(0, Math.min(100, progress * 100))
  
  // Create dynamic width classes based on progress
  const getProgressClass = () => {
    if (progressPercentage === 0) return 'w-0'
    if (progressPercentage >= 100) return 'w-full'
    if (progressPercentage >= 75) return 'w-3/4'
    if (progressPercentage >= 50) return 'w-1/2'
    if (progressPercentage >= 25) return 'w-1/4'
    return 'w-1/12'
  }
  
  return (
    <div className={`w-full bg-gray-200 dark:bg-gray-600 rounded-full h-2 overflow-hidden ${className}`}>
      <div className={`bg-blue-600 h-2 rounded-full transition-all duration-300 ${getProgressClass()}`} />
    </div>
  )
}