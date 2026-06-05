import { request } from '../request'

export const dataApi = {
  exportDataBackup: () => request('data:export'),
  previewDataBackupRestore: () => request('data:preview-restore'),
  restoreDataBackup: payload => request('data:restore', payload),
  listLocalBackups: () => request('data:local-backups'),
  createLocalBackup: () => request('data:local-backup-now'),
  previewLocalBackupRestore: payload =>
    request('data:local-backup-preview', payload),
  restoreLocalBackup: payload => request('data:local-backup-restore', payload),
  pushCloudBackup: payload => request('data:cloud-push', payload),
  inspectCloudBackup: payload => request('data:cloud-inspect', payload),
  previewCloudBackupRestore: payload => request('data:cloud-preview', payload),
  pullCloudBackup: payload => request('data:cloud-pull', payload)
}
