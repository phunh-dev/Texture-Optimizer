// Tool registry: one entry per tool. To plug in a real tool, point `load` at
// its module (e.g. `load: () => import('./resize')`) — that is the only change.
import type { ParseKeys } from 'i18next'
import {
  BoxesIcon,
  CropIcon,
  EraserIcon,
  Grid2x2CheckIcon,
  LayoutDashboardIcon,
  type LucideIcon,
  MaximizeIcon,
  ScalingIcon,
  TextCursorInputIcon,
} from 'lucide-react'
import type { ComponentType } from 'react'

import { atlasDefaults } from './atlas/defaults'
import { imageOpDefaults } from './_imageOps/defaults'
import { defaultBgRemoveParams } from './bgRemove/defaults'
import { defaultRenameParams } from './rename/defaults'
import { meshDefaults } from './meshPack/defaults'

export type ToolId = 'resize' | 'resolution' | 'trim' | 'potPad' | 'atlas' | 'rename' | 'bgRemove' | 'meshPack'

export interface ToolTabProps {
  tabId: string
}

export interface ToolDefinition {
  id: ToolId
  /** Key in the `tabs` namespace, e.g. 'tools.resize.title'. */
  titleKey: ParseKeys<'tabs'>
  descriptionKey: ParseKeys<'tabs'>
  icon: LucideIcon
  /** Lazily loaded tab component (code-split; only mounted while the tab is active). */
  load: () => Promise<{ default: ComponentType<ToolTabProps> }>
  /** Initial params for a new tab of this tool (also used by "Reset to defaults"). */
  defaultParams: () => Record<string, unknown>
}


export const tools: ToolDefinition[] = [
  { id: 'resize', titleKey: 'tools.resize.title', descriptionKey: 'tools.resize.description', icon: ScalingIcon, load: () => import('./resize'), defaultParams: imageOpDefaults.resize },
  { id: 'resolution', titleKey: 'tools.resolution.title', descriptionKey: 'tools.resolution.description', icon: Grid2x2CheckIcon, load: () => import('./resolution'), defaultParams: imageOpDefaults.resolution },
  { id: 'trim', titleKey: 'tools.trim.title', descriptionKey: 'tools.trim.description', icon: CropIcon, load: () => import('./trim'), defaultParams: imageOpDefaults.trim },
  { id: 'potPad', titleKey: 'tools.potPad.title', descriptionKey: 'tools.potPad.description', icon: MaximizeIcon, load: () => import('./potPad'), defaultParams: imageOpDefaults.potPad },
  { id: 'atlas', titleKey: 'tools.atlas.title', descriptionKey: 'tools.atlas.description', icon: LayoutDashboardIcon, load: () => import('./atlas'), defaultParams: atlasDefaults },
  { id: 'rename', titleKey: 'tools.rename.title', descriptionKey: 'tools.rename.description', icon: TextCursorInputIcon, load: () => import('./rename'), defaultParams: defaultRenameParams },
  { id: 'bgRemove', titleKey: 'tools.bgRemove.title', descriptionKey: 'tools.bgRemove.description', icon: EraserIcon, load: () => import('./bgRemove'), defaultParams: defaultBgRemoveParams },
  { id: 'meshPack', titleKey: 'tools.meshPack.title', descriptionKey: 'tools.meshPack.description', icon: BoxesIcon, load: () => import('./meshPack'), defaultParams: meshDefaults },
]

export function getTool(id: ToolId): ToolDefinition {
  const tool = tools.find((t) => t.id === id)
  if (!tool) throw new Error(`Unknown tool: ${id}`)
  return tool
}
