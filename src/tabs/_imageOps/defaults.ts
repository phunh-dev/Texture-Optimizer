// Default params of the image-op tabs (from their zod schemas = Rust defaults), for the registry.
import { schemaDefaults } from '@/components/ParamForm/validation'
import type { Params } from '@/stores/session'

import { potPadSchema } from '../potPad/schema'
import { resizeSchema } from '../resize/schema'
import { resolutionSchema } from '../resolution/schema'
import { trimSchema } from '../trim/schema'

export const imageOpDefaults: Record<'resize' | 'resolution' | 'trim' | 'potPad', () => Params> = {
  resize: () => schemaDefaults(resizeSchema),
  resolution: () => schemaDefaults(resolutionSchema),
  trim: () => schemaDefaults(trimSchema),
  potPad: () => schemaDefaults(potPadSchema),
}
