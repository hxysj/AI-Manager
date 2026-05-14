const chokidar = require('chokidar')

class FileWatcherService {
  constructor() {
    this.watcher = null
    this.timer = null
  }

  restart(paths, onRefresh) {
    this.stop()

    const uniquePaths = Array.from(new Set(paths.filter(Boolean)))

    if (!uniquePaths.length) {
      return
    }

    this.watcher = chokidar.watch(uniquePaths, {
      ignoreInitial: true,
      depth: 6,
      followSymlinks: false
    })

    const trigger = changedPath => {
      if (changedPath.endsWith('prompt.md')) {
        return
      }

      clearTimeout(this.timer)
      this.timer = setTimeout(() => {
        onRefresh()
      }, 250)
    }

    this.watcher.on('add', trigger)
    this.watcher.on('addDir', trigger)
    this.watcher.on('change', trigger)
    this.watcher.on('unlink', trigger)
    this.watcher.on('unlinkDir', trigger)
  }

  stop() {
    clearTimeout(this.timer)
    this.timer = null

    if (this.watcher) {
      this.watcher.close()
      this.watcher = null
    }
  }
}

module.exports = {
  FileWatcherService
}
