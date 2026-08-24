import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface Item {
  id: number
  kind: 'text' | 'image' | 'files'
  preview: string | null
  image_path: string | null
  thumb_path: string | null
  file_paths: string[] | null
  source_app: string | null
  source_icon_path: string | null
  pinned: boolean
  last_copied_at: number
  byte_size: number
  has_rich: boolean
}

export const listItems = (query: string) => invoke<Item[]>('list_items', { query })
export const selectItem = (id: number) => invoke<void>('select_item', { id })
export const togglePin = (id: number) => invoke<boolean>('toggle_pin', { id })
export const deleteItem = (id: number) => invoke<void>('delete_item', { id })
export const clearAll = () => invoke<void>('clear_all')
export const hideBar = () => invoke<void>('hide_bar')
export const axTrusted = () => invoke<boolean>('ax_trusted')

export const fileSrc = (path: string) => convertFileSrc(path)

export const onBarShown = (fn: () => void): Promise<UnlistenFn> => listen('bar-shown', fn)
export const onBarHiding = (fn: () => void): Promise<UnlistenFn> => listen('bar-hiding', fn)
export const onItemsChanged = (fn: () => void): Promise<UnlistenFn> => listen('items-changed', fn)

export function timeAgo(ms: number): string {
  const s = Math.max(0, Math.floor((Date.now() - ms) / 1000))
  if (s < 5) return 'now'
  if (s < 60) return `${s}s`
  const m = Math.floor(s / 60)
  if (m < 60) return `${m}m`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h}h`
  const d = Math.floor(h / 24)
  return `${d}d`
}

export function baseName(path: string): string {
  return path.replace(/\/+$/, '').split('/').pop() ?? path
}
