import { create } from 'zustand'

interface UiState {
  /** Files are being dragged over the window (highlight the active grid). */
  dragOver: boolean
  toolPickerOpen: boolean
  settingsOpen: boolean
  setDragOver: (value: boolean) => void
  setToolPickerOpen: (open: boolean) => void
  setSettingsOpen: (open: boolean) => void
}

export const useUi = create<UiState>()((set) => ({
  dragOver: false,
  toolPickerOpen: false,
  settingsOpen: false,
  setDragOver: (dragOver) => set({ dragOver }),
  setToolPickerOpen: (toolPickerOpen) => set({ toolPickerOpen }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
}))
