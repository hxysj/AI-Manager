const fs = require("node:fs/promises")
const path = require("node:path")
const crypto = require("node:crypto")
const { execFile } = require("node:child_process")
const { promisify } = require("node:util")

const execFileAsync = promisify(execFile)
const emptyTreeHash = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"

function normalizeProjectPath(projectPath) {
  return path.resolve(projectPath)
}

function createArchiveId(repoId, branchName, commitHash) {
  const seed = `${repoId}:${branchName}:${commitHash}:${Date.now()}`
  return crypto.createHash("sha1").update(seed).digest("hex").slice(0, 16)
}

function createStashArchiveId(repoId, stashHash) {
  const seed = `${repoId}:stash:${stashHash}:${Date.now()}`
  return crypto.createHash("sha1").update(seed).digest("hex").slice(0, 16)
}

async function runGitRaw(args, cwd) {
  const result = await execFileAsync("git", args, {
    cwd,
    windowsHide: true,
    maxBuffer: 1024 * 1024 * 20
  })

  return result.stdout
}

async function runGit(args, cwd) {
  return (await runGitRaw(args, cwd)).trim()
}

async function pathExists(targetPath) {
  try {
    await fs.access(targetPath)
    return true
  } catch (error) {
    if (error.code === "ENOENT") {
      return false
    }

    throw error
  }
}

async function readJson(filePath, fallbackValue) {
  try {
    return JSON.parse(await fs.readFile(filePath, "utf8"))
  } catch (error) {
    if (error.code === "ENOENT") {
      return fallbackValue
    }

    throw error
  }
}

async function writeJson(filePath, value) {
  await fs.mkdir(path.dirname(filePath), { recursive: true })
  await fs.writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8")
}

async function readGitPath(projectPath) {
  return runGit(["rev-parse", "--absolute-git-dir"], projectPath)
}

async function getOriginUrl(projectPath) {
  try {
    return await runGit(["remote", "get-url", "origin"], projectPath)
  } catch (error) {
    return ""
  }
}

async function branchExists(projectPath, branchName) {
  try {
    await runGit(["show-ref", "--verify", `refs/heads/${branchName}`], projectPath)
    return true
  } catch (error) {
    return false
  }
}

async function getBranchCommit(projectPath, branchName) {
  return runGit(["rev-parse", `refs/heads/${branchName}`], projectPath)
}

async function getCurrentBranch(projectPath) {
  return runGit(["branch", "--show-current"], projectPath)
}

async function getLocalBranchScan(projectPath) {
  const branchOutput = await runGit(
    [
      "for-each-ref",
      "--format=%(HEAD)%00%(refname:short)%00%(objectname)",
      "refs/heads"
    ],
    projectPath
  )

  if (!branchOutput) {
    return {
      currentBranch: "",
      branches: []
    }
  }

  let currentBranch = ""
  const branches = branchOutput.split("\n").map((line) => {
    const [head, name, commitHash] = line.split("\u0000")
    const isCurrent = head.trim() === "*"

    if (isCurrent) {
      currentBranch = name
    }

    return {
      name,
      commitHash,
      isCurrent
    }
  })

  return {
    currentBranch,
    branches
  }
}

function parseCommitLog(output) {
  if (!output) {
    return []
  }

  // git log --graph 会输出纯图形行，前端需要保留这些行来还原提交拓扑。
  return output.split("\n").map((line, index) => {
    if (!line.includes("\u0000")) {
      return {
        rowId: `graph-${index}`,
        hash: "",
        shortHash: "",
        subject: "",
        author: "",
        date: "",
        graph: line.trimEnd(),
        isGraphOnly: true,
        checkStatus: "none",
        checkTargetBranch: ""
      }
    }

    const [hashText, shortHash, subject, author, date] = line.split("\u0000")
    const hashMatch = hashText.match(/([0-9a-f]{40})$/)

    if (!hashMatch) {
      throw new Error("提交日志解析失败")
    }

    const hash = hashMatch[1]

    return {
      rowId: hash,
      hash,
      shortHash,
      subject,
      author,
      date,
      graph: hashText.slice(0, hashText.length - hash.length).trimEnd(),
      isGraphOnly: false,
      checkStatus: "none",
      checkTargetBranch: ""
    }
  })
}

async function getCommits(projectPath, branchName) {
  const output = await runGit(
    [
      "log",
      "--graph",
      branchName,
      "--date=iso-strict",
      "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
      "-n",
      "80"
    ],
    projectPath
  )

  return parseCommitLog(output)
}

async function findCommitBySubjectAndDate(projectPath, branchName, subject, date) {
  try {
    const output = await runGit(
      [
        "log",
        branchName,
        "--date=iso-strict",
        "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
        "--fixed-strings",
        "--grep",
        subject
      ],
      projectPath
    )

    return (
      parseCommitLog(output).find(
        (item) => item.subject === subject && item.date === date
      ) || null
    )
  } catch (error) {
    return null
  }
}

async function checkCommitsOnBranch(projectPath, branchName, commits) {
  const checkedCommits = []
  const matchedCommits = []
  const targetOutput = await runGit(
    [
      "log",
      branchName,
      "--date=iso-strict",
      "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad"
    ],
    projectPath
  )
  const targetCommits = parseCommitLog(targetOutput).filter(
    (item) => !item.isGraphOnly
  )
  const targetHashMap = new Map(
    targetCommits.map((commit) => [commit.hash, commit])
  )
  const targetSubjectDateMap = new Map()

  targetCommits.forEach((commit) => {
    const subjectDateKey = `${commit.subject}\u0000${commit.date}`

    if (!targetSubjectDateMap.has(subjectDateKey)) {
      targetSubjectDateMap.set(subjectDateKey, commit)
    }
  })

  // 先用提交哈希做精确合入判断，失败后再用标题做疑似匹配。
  for (const commit of commits) {
    if (commit.isGraphOnly) {
      checkedCommits.push(commit)
      continue
    }

    if (targetHashMap.has(commit.hash)) {
      checkedCommits.push({
        ...commit,
        checkStatus: "exists-hash",
        checkTargetBranch: branchName
      })
      matchedCommits.push({
        commitHash: commit.hash,
        subject: commit.subject,
        date: commit.date,
        targetBranchName: branchName,
        matchedBy: "hash",
        matchedCommit: {
          ...commit,
          checkStatus: "exists-hash",
          checkTargetBranch: branchName
        }
      })
      continue
    }

    const matchedCommit = targetSubjectDateMap.get(
      `${commit.subject}\u0000${commit.date}`
    )

    checkedCommits.push({
      ...commit,
      checkStatus: matchedCommit ? "exists-subject" : "missing",
      checkTargetBranch: branchName
    })

    if (matchedCommit) {
      matchedCommits.push({
        commitHash: commit.hash,
        subject: commit.subject,
        date: commit.date,
        targetBranchName: branchName,
        matchedBy: "subject-date",
        matchedCommit: {
          ...matchedCommit,
          checkStatus: "exists-subject",
          checkTargetBranch: branchName
        }
      })
    }
  }

  return {
    checkedCommits,
    matchedCommits
  }
}

async function checkCommitOnBranch(
  projectPath,
  branchName,
  commitHash,
  subject,
  date
) {
  try {
    await runGit(["merge-base", "--is-ancestor", commitHash, branchName], projectPath)

    const output = await runGit(
      [
        "show",
        "-s",
        "--date=iso-strict",
        "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
        commitHash
      ],
      projectPath
    )

    return {
      matchedBy: "hash",
      commit: parseCommitLog(output)[0]
    }
  } catch (error) {
    const matchedCommit = await findCommitBySubjectAndDate(
      projectPath,
      branchName,
      subject,
      date
    )

    if (!matchedCommit) {
      return null
    }

    return {
      matchedBy: "subject-date",
      commit: matchedCommit
    }
  }
}

function parseCommitFiles(output) {
  if (!output) {
    return []
  }

  return output
    .split("\n")
    .filter((line) => line.trim())
    .map((line) => {
      const parts = line.split("\t")
      const status = parts[0].slice(0, 1)

      if (status === "R" || status === "C") {
        return {
          status,
          path: parts[2],
          oldPath: parts[1]
        }
      }

      return {
        status,
        path: parts[1],
        oldPath: ""
      }
    })
    .filter((file) => file.path)
}

async function getCommitDetailByArgs(baseArgs, cwd, commitHash, filePath) {
  const infoOutput = await runGit(
    [
      ...baseArgs,
      "show",
      "-s",
      "--date=iso-strict",
      "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
      commitHash
    ],
    cwd
  )
  const [hash, shortHash, subject, author, date] = infoOutput.split("\u0000")
  const filesOutput = await runGit(
    [
      ...baseArgs,
      "show",
      "--format=",
      "--name-status",
      "--first-parent",
      "--find-renames",
      "--find-copies",
      commitHash
    ],
    cwd
  )
  const files = parseCommitFiles(filesOutput)
  const selectedFilePath = filePath || files[0]?.path || ""
  const patchArgs = [
    ...baseArgs,
    "show",
    "--format=",
    "--patch",
    "--first-parent",
    "--find-renames",
    "--find-copies",
    "--no-ext-diff",
    "--no-color",
    commitHash
  ]

  if (selectedFilePath) {
    patchArgs.push("--", selectedFilePath)
  }

  return {
    rowId: hash,
    hash,
    shortHash,
    subject,
    author,
    date,
    graph: "",
    isGraphOnly: false,
    checkStatus: "none",
    checkTargetBranch: "",
    files,
    selectedFilePath,
    patch: (await runGitRaw(patchArgs, cwd)).trimEnd()
  }
}

async function getStashes(projectPath) {
  const output = await runGit(
    [
      "stash",
      "list",
      "--date=iso-strict",
      "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad"
    ],
    projectPath
  )

  if (!output) {
    return []
  }

  return output.split("\n").map((line, index) => {
    const [hash, shortHash, subject, author, date] = line.split("\u0000")

    return {
      stashRef: `stash@{${index}}`,
      index,
      hash,
      shortHash,
      subject,
      author,
      date
    }
  })
}

async function getStashCommit(projectPath, stashRef) {
  return runGit(["rev-parse", stashRef], projectPath)
}

async function getStashCommitDetailByArgs(baseArgs, cwd, commitHash, filePath) {
  const infoOutput = await runGit(
    [
      ...baseArgs,
      "show",
      "-s",
      "--date=iso-strict",
      "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
      commitHash
    ],
    cwd
  )
  const [hash, shortHash, subject, author, date] = infoOutput.split("\u0000")
  const parentOutput = await runGit(
    [...baseArgs, "rev-list", "--parents", "-n", "1", commitHash],
    cwd
  )
  const parents = parentOutput.split(" ").slice(1)

  if (parents.length === 0) {
    throw new Error("stash 结构解析失败")
  }

  const trackedOutput = await runGit(
    [
      ...baseArgs,
      "diff",
      "--name-status",
      "--find-renames",
      "--find-copies",
      parents[0],
      commitHash
    ],
    cwd
  )
  const untrackedOutput =
    parents.length >= 3
      ? await runGit(
          [
            ...baseArgs,
            "diff",
            "--name-status",
            "--find-renames",
            "--find-copies",
            emptyTreeHash,
            parents[2]
          ],
          cwd
        )
      : ""
  const trackedFiles = parseCommitFiles(trackedOutput)
  const untrackedFiles = parseCommitFiles(untrackedOutput)
  const files = [...trackedFiles, ...untrackedFiles]
  const selectedFilePath = filePath || files[0]?.path || ""
  // stash 的第三个父节点保存未跟踪文件，展示 diff 时要和空树比较。
  const selectedInUntracked = untrackedFiles.some(
    (file) => file.path === selectedFilePath
  )
  const patchArgs = [
    ...baseArgs,
    "diff",
    "--patch",
    "--find-renames",
    "--find-copies",
    "--no-ext-diff",
    "--no-color",
    selectedInUntracked ? emptyTreeHash : parents[0],
    selectedInUntracked ? parents[2] : commitHash
  ]

  if (selectedFilePath) {
    patchArgs.push("--", selectedFilePath)
  }

  return {
    rowId: hash,
    hash,
    shortHash,
    subject,
    author,
    date,
    graph: "",
    isGraphOnly: false,
    checkStatus: "none",
    checkTargetBranch: "",
    files,
    selectedFilePath,
    patch: (await runGitRaw(patchArgs, cwd)).trimEnd()
  }
}

class GitToolService {
  constructor(paths, getRepos) {
    this.paths = paths
    this.getRepos = getRepos
    // 归档数据只写入工作区，不写回 Repo 本身的管理文件。
    this.dataDir = path.join(paths.workspaceRoot, "git-tool")
    this.projectsDir = path.join(this.dataDir, "projects")
    this.projectsFile = path.join(this.dataDir, "projects.json")
  }

  async init() {
    await fs.mkdir(this.projectsDir, { recursive: true })
  }

  getProjectDir(repoId) {
    return path.join(this.projectsDir, path.basename(String(repoId || "")))
  }

  getArchiveGitDir(repoId) {
    return path.join(this.getProjectDir(repoId), "archive.git")
  }

  getArchivesFile(repoId) {
    return path.join(this.getProjectDir(repoId), "archives.json")
  }

  getStashArchivesFile(repoId) {
    return path.join(this.getProjectDir(repoId), "stash-archives.json")
  }

  getCommitCheckCacheFile(repoId) {
    return path.join(this.getProjectDir(repoId), "commit-check-cache.json")
  }

  async readProjects() {
    return readJson(this.projectsFile, [])
  }

  async writeProjects(projects) {
    await writeJson(this.projectsFile, projects)
  }

  async readArchives(repoId) {
    return readJson(this.getArchivesFile(repoId), [])
  }

  async writeArchives(repoId, archives) {
    await writeJson(this.getArchivesFile(repoId), archives)
  }

  async readStashArchives(repoId) {
    return readJson(this.getStashArchivesFile(repoId), [])
  }

  async writeStashArchives(repoId, stashArchives) {
    await writeJson(this.getStashArchivesFile(repoId), stashArchives)
  }

  async readCommitCheckCache(repoId) {
    return readJson(this.getCommitCheckCacheFile(repoId), [])
  }

  async writeCommitCheckCache(repoId, commitCheckCache) {
    await writeJson(this.getCommitCheckCacheFile(repoId), commitCheckCache)
  }

  async getRepoProject(repoId) {
    const repo = this.getRepos().find((item) => item.id === repoId)

    if (!repo) {
      throw new Error("仓库不存在")
    }

    const projectPath = normalizeProjectPath(repo.localPath)
    const projects = await this.readProjects()
    const project = projects.find((item) => item.repoId === repoId) || {}

    return {
      repoId,
      name: repo.name,
      projectPath,
      gitPath: project.gitPath || "",
      originUrl: project.originUrl || repo.source || "",
      checkBranchName: project.checkBranchName || "",
      addedAt: project.addedAt || repo.createdAt || Date.now(),
      lastScannedAt: project.lastScannedAt || repo.lastSyncedAt || 0
    }
  }

  async saveProjectPatch(repoId, patch) {
    const repo = this.getRepos().find((item) => item.id === repoId)

    if (!repo) {
      throw new Error("仓库不存在")
    }

    const projects = await this.readProjects()
    const index = projects.findIndex((item) => item.repoId === repoId)
    const previous =
      index >= 0
        ? projects[index]
        : {
            repoId,
            name: repo.name,
            projectPath: normalizeProjectPath(repo.localPath),
            checkBranchName: "",
            addedAt: repo.createdAt || Date.now()
          }
    const nextProject = {
      ...previous,
      ...patch,
      repoId,
      name: repo.name,
      projectPath: normalizeProjectPath(repo.localPath)
    }

    if (index >= 0) {
      projects[index] = nextProject
    } else {
      projects.push(nextProject)
    }

    await this.writeProjects(projects)
    return nextProject
  }

  async resolveProject(repoId) {
    const project = await this.getRepoProject(repoId)
    const [gitPath, originUrl] = await Promise.all([
      readGitPath(project.projectPath),
      getOriginUrl(project.projectPath)
    ])

    return this.saveProjectPatch(repoId, {
      gitPath,
      originUrl,
      lastScannedAt: Date.now()
    })
  }

  async scanBranches(repoId) {
    const project = await this.getRepoProject(repoId)
    const [branchScan, archives, stashArchives] = await Promise.all([
      getLocalBranchScan(project.projectPath),
      this.listArchives(repoId),
      this.listStashArchives(repoId)
    ])

    return {
      project,
      currentBranch: branchScan.currentBranch,
      branches: branchScan.branches,
      archives,
      stashes: [],
      stashArchives
    }
  }

  async listCommits(repoId, branchName, options = {}) {
    const project = await this.getRepoProject(repoId)
    const commits = await getCommits(project.projectPath, branchName)

    if (
      options.skipCheck ||
      !project.checkBranchName ||
      project.checkBranchName === branchName
    ) {
      return commits
    }

    const commitCheckCache = await this.readCommitCheckCache(repoId)
    const commitCheckCacheMap = new Map(
      commitCheckCache.map((item) => [
        `${item.sourceBranchName}\u0000${item.commitHash}\u0000${item.subject}\u0000${item.date}\u0000${item.targetBranchName}`,
        item
      ])
    )
    const commitsForCheck = []
    const cachedCommits = commits.map((commit) => {
      if (commit.isGraphOnly) {
        return commit
      }

      const cachedCommit = commitCheckCacheMap.get(
        `${branchName}\u0000${commit.hash}\u0000${commit.subject}\u0000${commit.date}\u0000${project.checkBranchName}`
      )

      if (!cachedCommit) {
        commitsForCheck.push(commit)
        return commit
      }

      return {
        ...commit,
        checkStatus:
          cachedCommit.matchedBy === "hash" ? "exists-hash" : "exists-subject",
        checkTargetBranch: project.checkBranchName
      }
    })

    if (commitsForCheck.length === 0) {
      return cachedCommits
    }

    const checkResult = await checkCommitsOnBranch(
      project.projectPath,
      project.checkBranchName,
      commitsForCheck
    )
    let checkedCommitIndex = 0
    const checkedCommits = cachedCommits.map((commit) => {
      if (commit.isGraphOnly || commit.checkTargetBranch) {
        return commit
      }

      const checkedCommit = checkResult.checkedCommits[checkedCommitIndex]
      checkedCommitIndex += 1
      return checkedCommit
    })

    if (checkResult.matchedCommits.length) {
      const checkedAt = Date.now()

      checkResult.matchedCommits.forEach((item) => {
        commitCheckCacheMap.set(
          `${branchName}\u0000${item.commitHash}\u0000${item.subject}\u0000${item.date}\u0000${item.targetBranchName}`,
          {
            ...item,
            sourceBranchName: branchName,
            checkedAt
          }
        )
      })
      await this.writeCommitCheckCache(
        repoId,
        Array.from(commitCheckCacheMap.values())
      )
    }

    return checkedCommits
  }

  async updateCheckBranch(repoId, checkBranchName) {
    return this.saveProjectPatch(repoId, {
      checkBranchName: String(checkBranchName || "")
    })
  }

  async clearCommitCheckCache(repoId, sourceBranchName, targetBranchName) {
    await this.getRepoProject(repoId)
    const commitCheckCache = await this.readCommitCheckCache(repoId)
    const nextCommitCheckCache = commitCheckCache.filter(
      (item) =>
        item.sourceBranchName !== sourceBranchName ||
        item.targetBranchName !== targetBranchName
    )

    await this.writeCommitCheckCache(repoId, nextCommitCheckCache)
    return commitCheckCache.length - nextCommitCheckCache.length
  }

  async checkCommitOnBranch(
    repoId,
    sourceBranchName,
    targetBranchName,
    commitHash,
    subject,
    date
  ) {
    const project = await this.getRepoProject(repoId)
    const commitCheckCache = await this.readCommitCheckCache(repoId)
    const commitCheckCacheMap = new Map(
      commitCheckCache.map((item) => [
        `${item.sourceBranchName}\u0000${item.commitHash}\u0000${item.subject}\u0000${item.date}\u0000${item.targetBranchName}`,
        item
      ])
    )
    const cachedCommit = commitCheckCacheMap.get(
      `${sourceBranchName}\u0000${commitHash}\u0000${subject}\u0000${date}\u0000${targetBranchName}`
    )

    if (cachedCommit) {
      return {
        matchedBy: cachedCommit.matchedBy,
        commit: cachedCommit.matchedCommit
      }
    }

    const matchedResult = await checkCommitOnBranch(
      project.projectPath,
      targetBranchName,
      commitHash,
      subject,
      date
    )

    if (matchedResult) {
      commitCheckCacheMap.set(
        `${sourceBranchName}\u0000${commitHash}\u0000${subject}\u0000${date}\u0000${targetBranchName}`,
        {
          sourceBranchName,
          commitHash,
          subject,
          date,
          targetBranchName,
          matchedBy: matchedResult.matchedBy,
          matchedCommit: matchedResult.commit,
          checkedAt: Date.now()
        }
      )
      await this.writeCommitCheckCache(
        repoId,
        Array.from(commitCheckCacheMap.values())
      )
    }

    return matchedResult
  }

  async getCommitDetail(repoId, commitHash, filePath) {
    const project = await this.getRepoProject(repoId)
    return getCommitDetailByArgs([], project.projectPath, commitHash, filePath)
  }

  async ensureArchiveGit(repoId) {
    const archiveGitDir = this.getArchiveGitDir(repoId)

    if (await pathExists(path.join(archiveGitDir, "HEAD"))) {
      return archiveGitDir
    }

    // 每个 Repo 使用独立 bare 仓库存放分支和 stash 归档引用。
    await fs.mkdir(path.dirname(archiveGitDir), { recursive: true })
    await runGit(["init", "--bare", archiveGitDir], this.dataDir)
    return archiveGitDir
  }

  async archiveBranch(repoId, branchName) {
    const project = await this.getRepoProject(repoId)
    const currentBranch = await getCurrentBranch(project.projectPath)

    if (branchName === currentBranch) {
      throw new Error("当前 checkout 分支不能归档，请先切换到其他分支")
    }

    if (!(await branchExists(project.projectPath, branchName))) {
      throw new Error("本地分支不存在")
    }

    const commitHash = await getBranchCommit(project.projectPath, branchName)
    const archiveGitDir = await this.ensureArchiveGit(repoId)
    const archiveId = createArchiveId(repoId, branchName, commitHash)
    const archiveRef = `refs/archive/branches/${archiveId}`

    // 先把分支引用写入归档仓库，校验成功后才删除本地分支。
    await runGit(
      [
        "--git-dir",
        archiveGitDir,
        "fetch",
        "--no-tags",
        project.projectPath,
        `refs/heads/${branchName}:${archiveRef}`
      ],
      this.dataDir
    )

    const archivedCommitHash = await runGit(
      ["--git-dir", archiveGitDir, "rev-parse", archiveRef],
      this.dataDir
    )

    if (archivedCommitHash !== commitHash) {
      throw new Error("归档提交校验失败")
    }

    await runGit(["branch", "-D", branchName], project.projectPath)

    const archive = {
      archiveId,
      repoId,
      projectPath: project.projectPath,
      branchName,
      commitHash,
      archiveRef,
      archivedAt: Date.now(),
      restoredAt: 0
    }
    const archives = await this.readArchives(repoId)

    archives.unshift(archive)
    await this.writeArchives(repoId, archives)
    return archive
  }

  async listArchives(repoId) {
    return this.readArchives(repoId)
  }

  async listArchiveCommits(archiveId) {
    const { repoId, archive } = await this.findArchive(archiveId)
    const archiveGitDir = this.getArchiveGitDir(repoId)
    const output = await runGit(
      [
        "--git-dir",
        archiveGitDir,
        "log",
        archive.archiveRef,
        "--date=iso-strict",
        "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
        "-n",
        "80"
      ],
      this.dataDir
    )

    return parseCommitLog(output)
  }

  async getArchiveCommitDetail(archiveId, commitHash, filePath) {
    const { repoId } = await this.findArchive(archiveId)

    return getCommitDetailByArgs(
      ["--git-dir", this.getArchiveGitDir(repoId)],
      this.dataDir,
      commitHash,
      filePath
    )
  }

  async restoreArchive(archiveId, targetBranchName) {
    if (!targetBranchName) {
      throw new Error("请输入恢复分支名")
    }

    const { repoId, project, archive, archives } = await this.findArchive(archiveId)

    if (await branchExists(project.projectPath, targetBranchName)) {
      throw new Error("目标分支名已存在，请输入新的分支名")
    }

    const archiveGitDir = this.getArchiveGitDir(repoId)

    await runGit(
      [
        "fetch",
        "--no-tags",
        archiveGitDir,
        `${archive.archiveRef}:refs/heads/${targetBranchName}`
      ],
      project.projectPath
    )

    const restoredCommitHash = await getBranchCommit(
      project.projectPath,
      targetBranchName
    )

    if (restoredCommitHash !== archive.commitHash) {
      throw new Error("恢复提交校验失败")
    }

    await runGit(
      ["--git-dir", archiveGitDir, "update-ref", "-d", archive.archiveRef],
      this.dataDir
    )
    await this.writeArchives(
      repoId,
      archives.filter((item) => item.archiveId !== archiveId)
    )
    return archive
  }

  async deleteArchive(archiveId) {
    const { repoId, archive, archives } = await this.findArchive(archiveId)
    const archiveGitDir = this.getArchiveGitDir(repoId)

    await runGit(
      ["--git-dir", archiveGitDir, "update-ref", "-d", archive.archiveRef],
      this.dataDir
    )

    const nextArchives = archives.filter((item) => item.archiveId !== archiveId)
    await this.writeArchives(repoId, nextArchives)
    return nextArchives
  }

  async listStashes(repoId) {
    const project = await this.getRepoProject(repoId)
    return getStashes(project.projectPath)
  }

  async listStashArchives(repoId) {
    return this.readStashArchives(repoId)
  }

  async getStashDetail(repoId, stashHash, filePath) {
    if (!stashHash) {
      throw new Error("请选择要查看的 stash")
    }

    const project = await this.getRepoProject(repoId)
    const stash = (await getStashes(project.projectPath)).find(
      (item) => item.hash === stashHash
    )

    if (!stash) {
      throw new Error("stash 记录不存在，请刷新后重试")
    }

    return getStashCommitDetailByArgs([], project.projectPath, stash.hash, filePath)
  }

  async getStashArchiveDetail(stashArchiveId, filePath) {
    const { repoId, stashArchive } = await this.findStashArchive(stashArchiveId)

    return getStashCommitDetailByArgs(
      ["--git-dir", this.getArchiveGitDir(repoId)],
      this.dataDir,
      stashArchive.commitHash,
      filePath
    )
  }

  async archiveStash(repoId, stashRef, stashHash) {
    if (!stashHash) {
      throw new Error("请选择要归档的 stash")
    }

    const project = await this.getRepoProject(repoId)
    const stash = (await getStashes(project.projectPath)).find(
      (item) => item.hash === stashHash
    )

    if (!stash) {
      throw new Error("stash 记录不存在，请刷新后重试")
    }

    const commitHash = await getStashCommit(project.projectPath, stash.stashRef)

    if (commitHash !== stash.hash) {
      throw new Error("stash 记录已变化，请刷新后重试")
    }

    const archiveGitDir = await this.ensureArchiveGit(repoId)
    const stashArchiveId = createStashArchiveId(repoId, commitHash)
    const archiveRef = `refs/archive/stashes/${stashArchiveId}`

    // stash drop 不可逆，所以先把 stash commit 写入归档仓库并校验。
    await runGit(
      [
        "--git-dir",
        archiveGitDir,
        "fetch",
        "--no-tags",
        project.projectPath,
        `${commitHash}:${archiveRef}`
      ],
      this.dataDir
    )

    const archivedCommitHash = await runGit(
      ["--git-dir", archiveGitDir, "rev-parse", archiveRef],
      this.dataDir
    )

    if (archivedCommitHash !== commitHash) {
      throw new Error("stash 归档提交校验失败")
    }

    await runGit(["stash", "drop", stash.stashRef], project.projectPath)

    const stashArchive = {
      stashArchiveId,
      repoId,
      projectPath: project.projectPath,
      stashRef,
      message: stash.subject,
      commitHash,
      archiveRef,
      archivedAt: Date.now(),
      restoredAt: 0
    }
    const stashArchives = await this.readStashArchives(repoId)

    stashArchives.unshift(stashArchive)
    await this.writeStashArchives(repoId, stashArchives)
    return stashArchive
  }

  async restoreStashArchive(stashArchiveId) {
    const { repoId, project, stashArchive, stashArchives } =
      await this.findStashArchive(stashArchiveId)
    const archiveGitDir = this.getArchiveGitDir(repoId)
    const restoreRef = `refs/git-tool/stash-restore/${stashArchiveId}`

    // stash store 需要一个本地引用承载归档提交，恢复完成后立即清理。
    await runGit(
      [
        "fetch",
        "--no-tags",
        archiveGitDir,
        `${stashArchive.archiveRef}:${restoreRef}`
      ],
      project.projectPath
    )

    const restoredCommitHash = await runGit(
      ["rev-parse", restoreRef],
      project.projectPath
    )

    if (restoredCommitHash !== stashArchive.commitHash) {
      throw new Error("stash 恢复提交校验失败")
    }

    await runGit(
      ["stash", "store", "-m", stashArchive.message, restoreRef],
      project.projectPath
    )
    await runGit(["update-ref", "-d", restoreRef], project.projectPath)
    await runGit(
      ["--git-dir", archiveGitDir, "update-ref", "-d", stashArchive.archiveRef],
      this.dataDir
    )
    await this.writeStashArchives(
      repoId,
      stashArchives.filter((item) => item.stashArchiveId !== stashArchiveId)
    )
    return stashArchive
  }

  async deleteStashArchive(stashArchiveId) {
    const { repoId, stashArchive, stashArchives } =
      await this.findStashArchive(stashArchiveId)
    const archiveGitDir = this.getArchiveGitDir(repoId)

    await runGit(
      ["--git-dir", archiveGitDir, "update-ref", "-d", stashArchive.archiveRef],
      this.dataDir
    )

    const nextStashArchives = stashArchives.filter(
      (item) => item.stashArchiveId !== stashArchiveId
    )
    await this.writeStashArchives(repoId, nextStashArchives)
    return nextStashArchives
  }

  async findArchive(archiveId) {
    for (const repo of this.getRepos()) {
      const project = await this.getRepoProject(repo.id)
      const archives = await this.readArchives(repo.id)
      const archive = archives.find((item) => item.archiveId === archiveId)

      if (archive) {
        return {
          repoId: repo.id,
          project,
          archive,
          archives
        }
      }
    }

    throw new Error("未找到归档记录")
  }

  async findStashArchive(stashArchiveId) {
    for (const repo of this.getRepos()) {
      const project = await this.getRepoProject(repo.id)
      const stashArchives = await this.readStashArchives(repo.id)
      const stashArchive = stashArchives.find(
        (item) => item.stashArchiveId === stashArchiveId
      )

      if (stashArchive) {
        return {
          repoId: repo.id,
          project,
          stashArchive,
          stashArchives
        }
      }
    }

    throw new Error("未找到 stash 归档记录")
  }
}

module.exports = {
  GitToolService
}
