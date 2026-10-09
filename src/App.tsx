import { useTranslation } from 'react-i18next'

// Placeholder shell; replaced by the tabbed app shell (src/app).
export default function App() {
  const { t } = useTranslation('common')
  return (
    <main className="flex h-screen items-center justify-center bg-background text-foreground">
      <h1 className="text-2xl font-semibold">{t('app.title')}</h1>
    </main>
  )
}
