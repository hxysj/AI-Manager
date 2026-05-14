const fs = require('node:fs/promises')
const path = require('node:path')

async function getPathStat(targetPath) {
  try {
    return await fs.lstat(targetPath)
  } catch {
    return null
  }
}

async function pathExists(targetPath) {
  return Boolean(await getPathStat(targetPath))
}

async function removeManagedLink(targetPath) {
  const stat = await getPathStat(targetPath)

  if (!stat) {
    return
  }

  if (!stat.isSymbolicLink()) {
    throw new Error(`目标路径不是可管理的链接，已拒绝删除：${targetPath}`)
  }

  await fs.rm(targetPath, { recursive: true, force: true })
}

class LinkManager {
  constructor(cliDetectionService) {
    this.cliDetectionService = cliDetectionService
  }

  async getInstallState(skill, cliTarget) {
    if (!cliTarget.installed) {
      return {
        targetId: cliTarget.id,
        state: 'disabled',
        targetPath: path.join(cliTarget.skillsPath || '', skill.name),
        reason: 'CLI 未安装'
      }
    }

    const targetPath = path.join(cliTarget.skillsPath, skill.name)
    const targetStat = await getPathStat(targetPath)

    if (!targetStat) {
      return {
        targetId: cliTarget.id,
        state: 'not-installed',
        targetPath
      }
    }

    const sourceExists = await pathExists(skill.sourcePath)

    if (!sourceExists) {
      return {
        targetId: cliTarget.id,
        state: 'broken-link',
        targetPath
      }
    }

    try {
      if (!targetStat.isSymbolicLink()) {
        return {
          targetId: cliTarget.id,
          state: 'disabled',
          targetPath,
          reason: '目标路径已被真实目录占用'
        }
      }

      const resolvedTarget = await fs.realpath(targetPath)
      const resolvedSource = await fs.realpath(skill.sourcePath)

      if (resolvedTarget === resolvedSource) {
        return {
          targetId: cliTarget.id,
          state: 'installed',
          targetPath
        }
      }

      return {
        targetId: cliTarget.id,
        state: 'disabled',
        targetPath,
        reason: '目标路径已被其他内容占用'
      }
    } catch {
      return {
        targetId: cliTarget.id,
        state: 'broken-link',
        targetPath
      }
    }
  }

  async installSkill(skill, targetId) {
    const cliTarget = this.cliDetectionService.getAdapter(targetId)
    const detection = await cliTarget.detect()

    if (!detection.installed) {
      throw new Error(`${detection.name} 未安装，无法挂载 Skill`)
    }

    if (!(await pathExists(skill.sourcePath))) {
      throw new Error(`Skill 源目录不存在：${skill.sourcePath}`)
    }

    await fs.mkdir(detection.skillsPath, { recursive: true })
    const targetPath = path.join(detection.skillsPath, skill.name)
    const targetStat = await getPathStat(targetPath)

    if (targetStat) {
      if (!targetStat.isSymbolicLink()) {
        throw new Error(`目标路径已被真实目录占用，无法覆盖：${targetPath}`)
      }

      try {
        const resolvedTarget = await fs.realpath(targetPath)
        const resolvedSource = await fs.realpath(skill.sourcePath)

        if (resolvedTarget === resolvedSource) {
          return {
            targetId,
            targetPath
          }
        }
      } catch {}

      await removeManagedLink(targetPath)
    }

    await fs.symlink(skill.sourcePath, targetPath, 'junction')

    return {
      targetId,
      targetPath
    }
  }

  async uninstallSkill(skillName, targetId) {
    const cliTarget = this.cliDetectionService.getAdapter(targetId)
    const detection = await cliTarget.detect()

    if (!detection.skillsPath) {
      return
    }

    const targetPath = path.join(detection.skillsPath, skillName)
    await removeManagedLink(targetPath)
  }

  async repairSkill(skill, targetId) {
    return this.installSkill(skill, targetId)
  }
}

module.exports = {
  LinkManager
}
