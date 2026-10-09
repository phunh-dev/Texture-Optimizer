import { BoxIcon, FolderPlusIcon, ImageOffIcon, PackageIcon, PlusIcon, TriangleAlertIcon, UploadCloudIcon, XIcon } from 'lucide-react'
import type { KeyboardEvent, MouseEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/misc'
import { Tooltip } from '@/components/ui/tooltip'
import { useLooseT } from '@/i18n/loose'
import { translateError } from '@/lib/errors'
import { importFromFilePicker, importFromFolderPicker } from '@/lib/import'
import { thumbnailUrl } from '@/lib/ipc'
import type { ImportedFile } from '@/lib/ipc/types'
import { cn } from '@/lib/utils'
import { requireSession, useSession, type Params } from '@/stores/session'
import { useUi } from '@/stores/ui'

import type { MaterialInfo, MaterialStatus, ModelInfo, TextureInfo } from './ipc'
import { removeModelsWithUndo, useEnsureModelInfos, useModelInfos } from './models'
import { PACKED_STATUSES, predictStatus } from './preview'

const STATUS_STYLE: Record<MaterialStatus, string> = {
  inRange: 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-300',
  clamped: 'bg-sky-500/15 text-sky-700 dark:text-sky-300',
  wrapped: 'bg-sky-500/15 text-sky-700 dark:text-sky-300',
  repeated: 'bg-violet-500/15 text-violet-700 dark:text-violet-300',
  skipped: 'bg-amber-500/15 text-amber-700 dark:text-amber-300',
  tooManyTiles: 'bg-red-500/15 text-red-700 dark:text-red-300',
  noUvs: 'bg-red-500/15 text-red-700 dark:text-red-300',
  noTextures: 'bg-muted text-muted-foreground',
}

export function UvStatusBadge({ status, tiles }: { status: MaterialStatus; tiles?: [number, number] }) {
  const { t } = useTranslation('mesh')
  const label = status === 'repeated' ? t('status.repeated', { u: tiles?.[0] ?? 1, v: tiles?.[1] ?? 1 }) : t(`status.${status}`)
  return (
    <span className={cn('inline-flex shrink-0 items-center rounded-full px-1.5 py-0.5 text-[10px] font-medium', STATUS_STYLE[status])} data-status={status}>
      {label}
    </span>
  )
}

function channelLabel(t: ReturnType<typeof useLooseT>, channel: string): string {
  return channel.startsWith('other:') ? `${t('mesh:channels.other')} (${channel.slice(6)})` : t(`mesh:channels.${channel}`)
}

function TextureThumb({ texture }: { texture: TextureInfo }) {
  const t = useLooseT()
  const name = channelLabel(t, texture.channel)
  const tip = texture.embedded
    ? `${name} — ${t('mesh:list.embedded')}`
    : texture.exists
      ? `${name} — ${texture.rawPath}${texture.width ? ` (${texture.width}×${texture.height})` : ''}`
      : `${name} — ${t('mesh:list.missing', { path: texture.rawPath })}`
  return (
    <Tooltip content={tip}>
      <span
        className={cn(
          'relative flex size-9 shrink-0 items-center justify-center overflow-hidden rounded-md border bg-checker',
          texture.exists || texture.embedded ? 'border-border' : 'border-dashed border-destructive/70 bg-destructive/5',
        )}
        data-testid="texture-thumb"
        data-channel={texture.channel}
        data-missing={!texture.exists && !texture.embedded}
        aria-label={tip}
      >
        {texture.exists && !texture.embedded ? (
          <img src={thumbnailUrl({ path: texture.path, mtimeMs: texture.mtimeMs }, 'small')} alt="" className="size-full object-cover" draggable={false} />
        ) : texture.embedded ? (
          <PackageIcon className="size-4 text-muted-foreground" />
        ) : (
          <ImageOffIcon className="size-4 text-destructive" />
        )}
        <span className="absolute inset-x-0 bottom-0 truncate bg-black/55 px-0.5 text-center text-[8px] leading-3 text-white">{name}</span>
      </span>
    </Tooltip>
  )
}

function MaterialRow({ material, params }: { material: MaterialInfo; params: Params }) {
  const { t } = useTranslation('mesh')
  const { status, tiles } = predictStatus(material, params)
  return (
    <li className="space-y-1.5" data-testid="material-row">
      <div className="flex items-center gap-2">
        <span className={cn('min-w-0 flex-1 truncate text-xs font-medium', !PACKED_STATUSES.includes(status) && 'text-muted-foreground')}>
          {material.name || '—'}
        </span>
        {material.uvChannel != null && material.uvChannel > 0 ? (
          <span className="text-[10px] text-muted-foreground">{t('list.uvChannel', { channel: material.uvChannel })}</span>
        ) : null}
        <UvStatusBadge status={status} tiles={tiles} />
      </div>
      {material.textures.length > 0 ? (
        <div className="flex flex-wrap gap-1">
          {material.textures.map((tex) => (
            <TextureThumb key={tex.channel} texture={tex} />
          ))}
        </div>
      ) : (
        <p className="text-[11px] text-muted-foreground">{t('list.noTextures')}</p>
      )}
    </li>
  )
}

function ModelCard({ tabId, file, info, selected, params }: { tabId: string; file: ImportedFile; info?: ModelInfo; selected: boolean; params: Params }) {
  const { t } = useTranslation('mesh')
  const pending = useModelInfos((s) => s.pending[file.path] === true)
  const error = useModelInfos((s) => s.errors[file.path])

  const onSelect = (e: MouseEvent | KeyboardEvent) => {
    const mode = e.shiftKey ? 'range' : e.ctrlKey || e.metaKey ? 'toggle' : 'replace'
    requireSession(tabId).getState().select(file.id, mode)
  }

  return (
    <article
      role="listitem"
      tabIndex={0}
      aria-selected={selected}
      aria-label={file.name}
      data-testid="model-card"
      data-model-id={file.id}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          onSelect(e)
        }
        if (e.key === 'Delete') removeModelsWithUndo(tabId, [file.id])
      }}
      className={cn(
        'group relative flex flex-col gap-3 rounded-xl border bg-card p-3 text-left shadow-sm outline-none transition-all',
        'hover:border-primary/40 focus-visible:ring-2 focus-visible:ring-ring/50',
        selected ? 'border-primary ring-2 ring-primary/30' : 'border-border',
      )}
    >
      <header className="flex items-start gap-2 pr-6">
        <div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-accent text-primary">
          <BoxIcon className="size-4" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <h3 className="truncate text-sm font-semibold" title={file.path}>
              {file.name}
            </h3>
            <span className="shrink-0 rounded bg-muted px-1 py-0.5 font-mono text-[10px] font-semibold uppercase text-muted-foreground" data-testid="format-badge">
              {file.ext}
            </span>
          </div>
          {info ? (
            <p className="truncate text-[11px] text-muted-foreground">
              {t('list.meshes', { count: info.meshCount })} · {t('list.vertices', { count: info.vertexCount })} ·{' '}
              {t('list.materials', { count: info.materials.length })}
            </p>
          ) : null}
        </div>
      </header>

      <Tooltip content={t('list.remove', { name: file.name })}>
        <button
          type="button"
          aria-label={t('list.remove', { name: file.name })}
          onClick={(e) => {
            e.stopPropagation()
            removeModelsWithUndo(tabId, [file.id])
          }}
          className="absolute right-2 top-2 flex size-6 items-center justify-center rounded-full text-muted-foreground opacity-70 transition hover:bg-destructive hover:text-white group-hover:opacity-100"
        >
          <XIcon className="size-3.5" />
        </button>
      </Tooltip>

      {info ? (
        <>
          {info.warnings.length > 0 ? (
            <Tooltip content={info.warnings.slice(0, 5).map((w) => translateError(w)).join('\n')}>
              <span className="inline-flex w-fit items-center gap-1 rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-300" data-testid="model-warnings">
                <TriangleAlertIcon className="size-3" />
                {t('list.warnings', { count: info.warnings.length })}
              </span>
            </Tooltip>
          ) : null}
          <ul className="space-y-2.5">
            {info.materials.map((m) => (
              <MaterialRow key={m.index} material={m} params={params} />
            ))}
          </ul>
        </>
      ) : error ? (
        <p className="text-xs text-destructive">{translateError(error)}</p>
      ) : (
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
          {pending ? <Spinner className="size-3" /> : null}
          {pending ? t('list.loading') : t('list.unavailable')}
        </p>
      )}
    </article>
  )
}

function EmptyModels({ tabId, dragOver }: { tabId: string; dragOver: boolean }) {
  const { t } = useTranslation('mesh')
  return (
    <div className="flex size-full items-center justify-center p-6" data-testid="mesh-empty">
      <div
        className={cn(
          'flex w-full max-w-2xl flex-col items-center gap-5 rounded-2xl border-2 border-dashed px-8 py-14 text-center transition-all duration-200',
          dragOver ? 'scale-[1.01] border-primary bg-primary/5' : 'border-border bg-card/40',
        )}
      >
        <div className={cn('flex size-16 items-center justify-center rounded-2xl', dragOver ? 'bg-primary text-primary-foreground' : 'bg-accent text-primary')}>
          <UploadCloudIcon className="size-8" strokeWidth={1.5} />
        </div>
        <div className="space-y-1.5">
          <h2 className="text-lg font-semibold">{dragOver ? t('list.dropActive') : t('list.dropTitle')}</h2>
          <p className="mx-auto max-w-md text-sm text-muted-foreground">{t('list.dropHint')}</p>
        </div>
        <div className="flex flex-wrap items-center justify-center gap-2">
          <Button onClick={() => void importFromFilePicker(tabId)}>
            <PlusIcon />
            {t('list.add')}
          </Button>
          <span className="text-xs text-muted-foreground">{t('list.or')}</span>
          <Button variant="outline" onClick={() => void importFromFolderPicker(tabId)}>
            <FolderPlusIcon />
            {t('list.addFolder')}
          </Button>
        </div>
      </div>
    </div>
  )
}

/** Model cards (replaces the image grid in this tab). */
export function ModelList({ tabId }: { tabId: string }) {
  const { t } = useTranslation('mesh')
  const files = useSession(tabId, (s) => s.files)
  const selectedIds = useSession(tabId, (s) => s.selectedIds)
  const params = useSession(tabId, (s) => s.params)
  const infos = useModelInfos((s) => s.infos)
  const dragOver = useUi((s) => s.dragOver)
  useEnsureModelInfos(files)

  if (files.length === 0) return <EmptyModels tabId={tabId} dragOver={dragOver} />

  return (
    <div className={cn('size-full overflow-y-auto p-3 transition-colors', dragOver && 'bg-primary/5')} data-testid="model-list">
      <div role="list" aria-label={t('list.count', { count: files.length })} className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
        {files.map((f) => (
          <ModelCard key={f.id} tabId={tabId} file={f} info={infos[f.path]} selected={selectedIds.includes(f.id)} params={params} />
        ))}
        <button
          type="button"
          onClick={() => void importFromFilePicker(tabId)}
          aria-label={t('list.add')}
          data-testid="add-model-card"
          className="flex min-h-32 flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-border text-muted-foreground transition hover:border-primary/50 hover:text-primary"
        >
          <PlusIcon className="size-6" />
          <span className="text-xs font-medium">{t('list.add')}</span>
        </button>
      </div>
    </div>
  )
}
