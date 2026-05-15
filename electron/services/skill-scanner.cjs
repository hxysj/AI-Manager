const fs = require("node:fs/promises")
const path = require("node:path")

const IGNORE_DIRS = new Set([
  ".git",
  "node_modules",
  "dist",
  "build",
  ".cache",
  "temp"
])

async function isDirectory(targetPath) {
  try {
    const stat = await fs.lstat(targetPath)
    return stat.isDirectory() && !stat.isSymbolicLink()
  } catch {
    return false
  }
}

class SkillScanner {
  async scanMany(rootItems) {
    const results = []

    for (const item of rootItems) {
      const exists = await isDirectory(item.rootPath)

      if (!exists) {
        continue
      }

      const scanned = await this.scanRoot(item.rootPath, item.repoId)
      results.push(...scanned)
    }

    return results
  }

  async scanRoot(rootPath, repoId = null) {
    const results = []

    const visit = async (currentPath, depth) => {
      if (depth > 6) {
        return
      }

      const skillManifest = path.join(currentPath, "SKILL.md")

      try {
        const manifestStat = await fs.lstat(skillManifest)

        if (manifestStat.isFile()) {
          results.push({
            rootPath,
            repoId,
            skillRoot: currentPath
          })
          return
        }
      } catch {}

      const entries = await fs.readdir(currentPath, { withFileTypes: true })

      for (const entry of entries) {
        if (!entry.isDirectory()) {
          continue
        }

        if (IGNORE_DIRS.has(entry.name)) {
          continue
        }

        const nextPath = path.join(currentPath, entry.name)
        const stat = await fs.lstat(nextPath)

        if (stat.isSymbolicLink()) {
          continue
        }

        await visit(nextPath, depth + 1)
      }
    }

    await visit(rootPath, 0)
    return results
  }
}

module.exports = {
  SkillScanner
}
