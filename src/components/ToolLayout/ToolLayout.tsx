import { EyeIcon, FolderPlusIcon, ImagePlusIcon, LayoutGridIcon, Trash2Icon } from 'lucide-react'
import { useMemo, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import type { z } from 'zod'

import { GridSizeToggle, ImageGrid } from '@/components/ImageGrid'
import { ParamForm, PresetBar, validateParams, type FieldDescriptor } from '@/components/ParamForm'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/misc'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { Tooltip } from '@/components/ui/tooltip'
import { clearFilesWithUndo } from '@/lib/history'
import { importFromFilePicker, importFromFolderPicker } from '@/lib/import'
import { runOp } from '@/lib/ipc'
import type { ImportedFile, OpRequest, OutputSettings } from '@/lib/ipc/types'
import { cn } from '@/lib/utils'
import { useJobs } from '@/stores/jobs'
import { requireSession, useSession, type Params } from '@/stores/session'
import { getTabInfo } from '@/stores/tabs'

import { HistoryButtons } from './HistoryButtons'
import { OutputSettingsPanel, outputProblem } from './OutputSettingsPanel'
import { RunPanel } from './RunPanel'

/** Snapshot handed to the request builder, custom runners and the preview slot. */
export interface ToolContext {
  tabId: string
  files: ImportedFile[]
  params: Params
  output: OutputSettings
  selectedIds: string[]
  /** First selected file (or the first file) — handy for previews. */
  focusFile: ImportedFile | null
}

export interface ToolLayoutProps {
  tabId: string
  /** Parameter controls (rendered by ParamForm). */
  fields?: FieldDescriptor[]
  /** zod schema validating `params`; Run is disabled while invalid. */
  schema?: z.ZodType
  /** Builds the single-image op request; Run then calls runOp(tabId, request, paths, output). */
  buildRequest?: (ctx: ToolContext) => OpRequest | null
  /** Custom job starter for tools that do not use runOp (atlas, rename, mesh). Must resolve with the job id. */
  run?: (ctx: ToolContext) => Promise<string>
  /** Optional preview; when given, the toolbar shows an Images / Preview switch. */
  preview?: (ctx: ToolContext) => ReactNode
  /** Extra content at the top of the side panel (above the parameters). */
  sidePanelTop?: ReactNode
  /** Extra content below the parameters. */
  sidePanelBottom?: ReactNode
  /** Show the shared output settings panel (default true). */
  showOutput?: boolean
  /** Show the preset bar (default true when there are fields). */
  showPresets?: boolean
  /** Custom Run button label (default "Process N images"). */
  runLabel?: string
  /** Extra tool-specific reason (already translated) that disables Run, checked after files/params. */
  runDisabledReason?: string | null
  /** Replaces the image grid in the main area (tools whose items are not images, e.g. 3D models). */
  content?: (ctx: ToolContext) => ReactNode
  /** Replaces the "N images" toolbar counter (already translated). */
  countLabel?: string
  /** Hides the S/M/L grid size toggle (default: shown with the image grid only). */
  showGridSize?: boolean
  /**
   * Replaces the bottom Run button / job progress area (tools whose action is
   * not a background job, e.g. the renamer's synchronous Rename + confirm).
   */
  runPanel?: ReactNode
}

type View = 'grid' | 'preview'

/** Reusable tab layout: toolbar, image grid (or preview), side panel with params, output and Run. */
export function ToolLayout({
  tabId,
  fields = [],
  schema,
  buildRequest,
  run,
  preview,
  sidePanelTop,
  sidePanelBottom,
  showOutput = true,
  showPresets,
  runLabel,
  runDisabledReason,
  content,
  countLabel,
  showGridSize,
  runPanel,
}: ToolLayoutProps) {
  const { t } = useTranslation('common')
  const toolId = getTabInfo(tabId)?.toolId ?? 'unknown'
  const files = useSession(tabId, (s) => s.files)
  const params = useSession(tabId, (s) => s.params)
  const output = useSession(tabId, (s) => s.output)
  const selectedIds = useSession(tabId, (s) => s.selectedIds)
  const previewFlag = useSession(tabId, (s) => s.uiFlags.preview === true)
  const view: View = preview && previewFlag ? 'preview' : 'grid'
  const errors = useMemo(() => validateParams(schema, params), [schema, params])

  const ctx = (): ToolContext => {
    const s = requireSession(tabId).getState()
    const focusFile = s.files.find((f) => f.id === s.selectedIds[0]) ?? s.files[0] ?? null
    return { tabId, files: s.files, params: s.params, output: s.output, selectedIds: s.selectedIds, focusFile }
  }

  const problem = outputProblem(output)
  let disabledReason: string | null = null
  if (!run && !buildRequest) disabledReason = t('run.notAvailable')
  else if (files.length === 0) disabledReason = t('run.noFiles')
  else if (Object.keys(errors).length > 0) disabledReason = t('run.invalidParams')
  else if (showOutput && problem) disabledReason = t(problem)
  else if (runDisabledReason) disabledReason = runDisabledReason

  const onRun = () => {
    const c = ctx()
    if (run) {
      void useJobs.getState().start(tabId, c.files.length, () => run(c))
      return
    }
    const request = buildRequest?.(c)
    if (!request) return
    void useJobs.getState().start(tabId, c.files.length, () =>
      runOp(
        tabId,
        request,
        c.files.map((f) => f.path),
        c.output,
      ),
    )
  }

  const changeView = (v: View) => requireSession(tabId).getState().setUiFlag('preview', v === 'preview')

  const presetsVisible = showPresets ?? fields.length > 0
  const focusFile = files.find((f) => f.id === selectedIds[0]) ?? files[0] ?? null

  return (
    <div className="flex size-full min-h-0" data-testid="tool-layout">
      <section className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-12 shrink-0 items-center gap-1.5 border-b border-border bg-card/50 px-3">
          <Button variant="secondary" size="sm" onClick={() => void importFromFilePicker(tabId)}>
            <ImagePlusIcon />
            <span className="max-lg:hidden">{t('actions.addFiles')}</span>
          </Button>
          <Tooltip content={t('actions.addFolder')}>
            <Button variant="ghost" size="icon-sm" aria-label={t('actions.addFolder')} onClick={() => void importFromFolderPicker(tabId)}>
              <FolderPlusIcon />
            </Button>
          </Tooltip>
          <Separator orientation="vertical" className="mx-1" />
          <HistoryButtons tabId={tabId} />
          <Separator orientation="vertical" className="mx-1" />
          <span className="truncate text-xs tabular-nums text-muted-foreground" data-testid="file-count">
            {countLabel ?? t('grid.count', { count: files.length })}
            {selectedIds.length > 0 ? (
              <span className="ml-1.5 rounded-full bg-primary/10 px-1.5 py-0.5 font-medium text-primary">
                {t('grid.selected', { count: selectedIds.length })}
              </span>
            ) : null}
          </span>

          <div className="ml-auto flex items-center gap-1.5">
            {preview ? (
              <ToggleGroup type="single" value={view} aria-label={t('views.label')} onValueChange={(v) => v && changeView(v as View)}>
                <ToggleGroupItem value="grid" aria-label={t('views.grid')}>
                  <LayoutGridIcon />
                  <span className="max-xl:hidden">{t('views.grid')}</span>
                </ToggleGroupItem>
                <ToggleGroupItem value="preview" aria-label={t('views.preview')}>
                  <EyeIcon />
                  <span className="max-xl:hidden">{t('views.preview')}</span>
                </ToggleGroupItem>
              </ToggleGroup>
            ) : null}
            {view === 'grid' && (showGridSize ?? !content) ? <GridSizeToggle tabId={tabId} /> : null}
            <Tooltip content={t('actions.clearAll')}>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t('actions.clearAll')}
                disabled={files.length === 0}
                onClick={() => clearFilesWithUndo(tabId)}
                className="hover:text-destructive"
              >
                <Trash2Icon />
              </Button>
            </Tooltip>
          </div>
        </div>

        <div className="relative min-h-0 flex-1">
          {view === 'preview' && preview ? (
            <div className="size-full p-3">
              {preview({ tabId, files, params, output, selectedIds, focusFile })}
            </div>
          ) : content ? (
            content({ tabId, files, params, output, selectedIds, focusFile })
          ) : (
            <ImageGrid tabId={tabId} />
          )}
        </div>
      </section>

      <aside className="flex w-[340px] shrink-0 flex-col border-l border-border bg-card/40">
        <div className="min-h-0 flex-1 space-y-5 overflow-y-auto p-4">
          {sidePanelTop}
          <PanelSection title={t('params.title')} action={presetsVisible ? <PresetBar tabId={tabId} toolId={toolId} /> : null}>
            <ParamForm tabId={tabId} fields={fields} schema={schema} />
          </PanelSection>
          {sidePanelBottom}
          {showOutput ? (
            <PanelSection title={t('output.title')}>
              <OutputSettingsPanel tabId={tabId} />
            </PanelSection>
          ) : null}
        </div>
        <div className="shrink-0 border-t border-border bg-card/80 p-4 backdrop-blur">
          {runPanel ?? <RunPanel tabId={tabId} fileCount={files.length} disabledReason={disabledReason} onRun={onRun} label={runLabel} />}
        </div>
      </aside>
    </div>
  )
}

function PanelSection({ title, action, children, className }: { title: string; action?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={cn('space-y-3', className)}>
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{title}</h2>
        {action ? <div className="flex min-w-0 max-w-[60%] flex-1 justify-end">{action}</div> : null}
      </div>
      {children}
    </section>
  )
}
