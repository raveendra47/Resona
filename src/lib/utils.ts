/**
 * Utility functions for Resona player
 */

/**
 * Convert hex color to rgba string
 * @param hex - Hex color code (e.g., '#FF0000')
 * @param alpha - Alpha value (0-1)
 * @returns rgba string
 */
export function hexToRgba(hex: string, alpha: number): string {
  const r = parseInt(hex.slice(1, 3), 16)
  const g = parseInt(hex.slice(3, 5), 16)
  const b = parseInt(hex.slice(5, 7), 16)
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

/**
 * Get display title for a track
 * @param track - Track object
 * @returns Display title
 */
export function getTrackDisplayTitle(track: any): string {
  return track?.title || 'Unknown Track'
}

/**
 * Format milliseconds to MM:SS format
 * @param ms - Milliseconds
 * @returns Formatted time string
 */
export function formatTime(ms: number): string {
  if (!isFinite(ms)) return '0:00'
  const s = Math.floor(ms)
  const m = Math.floor(s / 60)
  const sec = s % 60
  return `${m}:${sec.toString().padStart(2, '0')}`
}

/**
 * Clamp a number between min and max
 * @param num - Number to clamp
 * @param min - Minimum value
 * @param max - Maximum value
 * @returns Clamped number
 */
export function clamp(num: number, min: number, max: number): number {
  return Math.min(Math.max(num, min), max)
}
