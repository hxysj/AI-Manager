export function fileUrl(service, file, sessionId, action = "preview") {
  if (!service?.running || !service.accessUrl || !file?.id) return ""
  try {
    const access = new URL(service.accessUrl)
    const url = new URL(`/api/files/${action}`, access.origin)
    url.search = new URLSearchParams({
      id: file.id,
      token: access.searchParams.get("token") || "",
      deviceId: "desktop",
      sessionId: sessionId || file.sessionId || ""
    }).toString()
    return url.href
  } catch {
    return ""
  }
}

export function fileKind(file) {
  const mime = String(file?.mimeType || file?.type || "").toLowerCase()
  if (mime.startsWith("image/")) return "image"
  if (mime.startsWith("video/")) return "video"
  if (mime.startsWith("audio/")) return "audio"
  if (mime === "application/pdf") return "pdf"
  return "file"
}

export function formatFileSize(value) {
  if (value == null) return "本地文件"
  const bytes = Number(value) || 0
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${bytes} B`
}
