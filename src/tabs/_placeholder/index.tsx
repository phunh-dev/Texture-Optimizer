// Generic tab used until a tool's real module replaces its registry entry.
import { ToolLayout } from '@/components/ToolLayout'
import type { ToolTabProps } from '@/tabs/registry'

export default function PlaceholderTab({ tabId }: ToolTabProps) {
  return <ToolLayout tabId={tabId} fields={[]} />
}
