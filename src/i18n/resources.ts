// One JSON file per namespace and language. To add a namespace, create
// src/locales/{en,vi}/<ns>.json and register it here.
import en_common from '../locales/en/common.json'
import en_tabs from '../locales/en/tabs.json'
import en_errors from '../locales/en/errors.json'
import en_settings from '../locales/en/settings.json'
import en_resize from '../locales/en/resize.json'
import en_resolution from '../locales/en/resolution.json'
import en_trim from '../locales/en/trim.json'
import en_potpad from '../locales/en/potpad.json'
import en_atlas from '../locales/en/atlas.json'
import en_rename from '../locales/en/rename.json'
import en_bgremove from '../locales/en/bgremove.json'
import en_mesh from '../locales/en/mesh.json'
import vi_common from '../locales/vi/common.json'
import vi_tabs from '../locales/vi/tabs.json'
import vi_errors from '../locales/vi/errors.json'
import vi_settings from '../locales/vi/settings.json'
import vi_resize from '../locales/vi/resize.json'
import vi_resolution from '../locales/vi/resolution.json'
import vi_trim from '../locales/vi/trim.json'
import vi_potpad from '../locales/vi/potpad.json'
import vi_atlas from '../locales/vi/atlas.json'
import vi_rename from '../locales/vi/rename.json'
import vi_bgremove from '../locales/vi/bgremove.json'
import vi_mesh from '../locales/vi/mesh.json'

export const namespaces = ['common',  'tabs',  'errors',  'settings',  'resize',  'resolution',  'trim',  'potpad',  'atlas',  'rename',  'bgremove',  'mesh'] as const

export const resources = {
  en: {
    common: en_common,
    tabs: en_tabs,
    errors: en_errors,
    settings: en_settings,
    resize: en_resize,
    resolution: en_resolution,
    trim: en_trim,
    potpad: en_potpad,
    atlas: en_atlas,
    rename: en_rename,
    bgremove: en_bgremove,
    mesh: en_mesh,
  },
  vi: {
    common: vi_common,
    tabs: vi_tabs,
    errors: vi_errors,
    settings: vi_settings,
    resize: vi_resize,
    resolution: vi_resolution,
    trim: vi_trim,
    potpad: vi_potpad,
    atlas: vi_atlas,
    rename: vi_rename,
    bgremove: vi_bgremove,
    mesh: vi_mesh,
  },
} as const

export type Namespace = (typeof namespaces)[number]
