const fs = require("node:fs/promises")
const path = require("node:path")
const crypto = require("node:crypto")
const matter = require("gray-matter")

async function fileExists(targetPath) {
  try {
    await fs.access(targetPath)
    return true
  } catch {
    return false
  }
}

class MetadataParser {
  async parse(skillRoot, repoId = null) {
    const skillFile = path.join(skillRoot, "SKILL.md")
    const content = await fs.readFile(skillFile, "utf8")
    const parsed = matter(content)
    const metadata = parsed.data || {}
    const skillName = String(metadata.name || "").trim()

    if (!skillName) {
      throw new Error(
        `Missing required frontmatter field "name" in ${skillFile}`
      )
    }

    const entryValue = String(metadata.entry || "SKILL.md").trim() || "SKILL.md"
    const entryPath = path.join(skillRoot, entryValue)
    const iconCandidate = metadata.icon
      ? path.join(skillRoot, String(metadata.icon).trim())
      : null
    const fallbackIcon = path.join(skillRoot, "icon.png")
    const iconPath =
      iconCandidate && (await fileExists(iconCandidate))
        ? iconCandidate
        : (await fileExists(fallbackIcon))
          ? fallbackIcon
          : null
    const stat = await fs.stat(skillRoot)

    return {
      id: crypto
        .createHash("sha1")
        .update(skillRoot)
        .digest("hex")
        .slice(0, 16),
      name: skillName,
      description: metadata.description
        ? String(metadata.description).trim()
        : "",
      content: parsed.content.trim(),
      version: metadata.version ? String(metadata.version).trim() : "",
      author: metadata.author ? String(metadata.author).trim() : "",
      tags: Array.isArray(metadata.tags)
        ? metadata.tags.map((item) => String(item).trim()).filter(Boolean)
        : [],
      icon: iconPath,
      entry: entryValue,
      entryPath: (await fileExists(entryPath)) ? entryPath : skillFile,
      homepage: metadata.homepage ? String(metadata.homepage).trim() : "",
      repository: metadata.repository ? String(metadata.repository).trim() : "",
      repoId,
      sourcePath: skillRoot,
      installedTargets: [],
      createdAt: stat.birthtimeMs || Date.now(),
      updatedAt: stat.mtimeMs || Date.now()
    }
  }
}

module.exports = {
  MetadataParser
}
