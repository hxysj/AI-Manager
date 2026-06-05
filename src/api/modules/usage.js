import { request } from '../request'

export const usageApi = {
  getUsageStats: payload => request('usage:stats', payload),
  getSkillUsageStats: payload => request('skill-usage:stats', payload),
  getUsagePricing: () => request('usage:pricing'),
  saveUsagePricing: payload => request('usage:save-pricing', payload),
  syncUsage: payload => request('usage:sync', payload),
  exportUsageReportImage: payload => request('usage:export-image', payload)
}
