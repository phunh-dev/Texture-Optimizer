// Feature namespaces are filled by later agents, so keys coming from data
// (field descriptors, registry entries) are plain strings. These helpers
// translate such runtime keys without fighting the strict key types.
import { useTranslation } from 'react-i18next'

import i18n from '.'

export type LooseT = (key: string, options?: Record<string, unknown>) => string

/** Translate a runtime key such as 'resize:params.width.label'. */
export const tKey: LooseT = (key, options) => (i18n.t as unknown as LooseT)(key, options)

/** Hook form of tKey that re-renders on language change. */
export function useLooseT(): LooseT {
  const { t } = useTranslation()
  return t as unknown as LooseT
}
