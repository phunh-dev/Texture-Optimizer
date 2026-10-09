import { Toaster as Sonner, type ToasterProps } from 'sonner'

import { useResolvedTheme } from '@/stores/settings'

export function Toaster(props: ToasterProps) {
  const theme = useResolvedTheme()
  return (
    <Sonner
      theme={theme}
      position="bottom-right"
      closeButton
      richColors
      toastOptions={{ classNames: { toast: 'font-sans' } }}
      {...props}
    />
  )
}

export { toast } from 'sonner'
