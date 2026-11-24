'use client'

import { QueryClientProvider } from '@tanstack/react-query'
import { useEffect, useRef, type ReactNode } from 'react'
import { I18nextProvider } from 'react-i18next'

import { StartupView } from '@/components/app/StartupView'
// Booharu: no Updater import, as StartupBoundary never mounts it.
import ClientOnly from '@/components/ClientOnly'
import { useBooharuAgentChatEvents } from '@/lib/booharu/agentChat' // Booharu: the agent's chat.
import { useBooharuJobEvents } from '@/lib/booharu/booharuJobs' // Booharu: its reads going.
import { useBooharuBrush } from '@/lib/booharu/brush' // Booharu: a brush for each brush tool.
import { useBooharuRunGroups } from '@/lib/booharu/chime' // Booharu: carrying-on groups' runs.
import { useBooharuContextChanges } from '@/lib/booharu/context' // Booharu: glossary changes.
import { useBooharuNotices } from '@/lib/booharu/notices' // Booharu: its notices, as toasts.
import { useBooharuQcEvents } from '@/lib/booharu/qc' // Booharu: the quality check's findings.
import { useTypingShortcuts } from '@/lib/booharu/typingKeys' // Booharu: Ctrl+B and Ctrl+I.
import { hear } from '@/lib/events'
import i18n from '@/lib/i18n'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { Toaster } from '@koharu/ui/components/toast'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

export function Providers({ children }: { children: ReactNode }) {
  const runtime = useRef({ active: false, bound: false })

  useEffect(() => {
    const lifecycle = runtime.current
    lifecycle.active = true
    if (!lifecycle.bound) {
      lifecycle.bound = true
      void hear(() => lifecycle.active).catch(() => undefined)
    }

    return () => {
      lifecycle.active = false
    }
  }, [])

  useEffect(() => {
    const setLanguage = (language: string) => {
      document.documentElement.lang = language
    }
    setLanguage(i18n.language)
    i18n.on('languageChanged', setLanguage)
    return () => i18n.off('languageChanged', setLanguage)
  }, [])

  useEffect(() => {
    // The webview is the app's window, not a browser tab: it applies no browser zoom, keeping
    // Ctrl+wheel for the app's handlers, and opens no browser menu on a right-click, where the
    // app shows its own menu or none. A text field keeps its edit menu, as Clip Studio Paint's do.
    const preventViewportScaling = (event: WheelEvent) => {
      if (event.ctrlKey) event.preventDefault()
    }
    const preventBrowserMenu = (event: MouseEvent) => {
      if (!(event.target instanceof Element && event.target.matches(':read-write'))) {
        event.preventDefault()
      }
    }

    window.addEventListener('wheel', preventViewportScaling, { capture: true, passive: false })
    window.addEventListener('contextmenu', preventBrowserMenu)
    return () => {
      window.removeEventListener('wheel', preventViewportScaling, { capture: true })
      window.removeEventListener('contextmenu', preventBrowserMenu)
    }
  }, [])

  useBooharuNotices() // Booharu: its own notices, such as why a drawn box added nothing.
  useBooharuRunGroups() // Booharu: which runs carry on a chapter together, so a group chimes once.
  useBooharuContextChanges() // Booharu: reads the series and the chapter again as they change.
  useBooharuJobEvents() // Booharu: follows each Read chapter going, which no run works on.
  useBooharuAgentChatEvents() // Booharu: keeps the agent's chat with the project, as it changes.
  useBooharuQcEvents() // Booharu: keeps the quality check's findings with the project, page by page.
  useTypingShortcuts() // Booharu: Ctrl+B and Ctrl+I make the characters typed bold or italic.
  useBooharuBrush() // Booharu: each brush tool's own size, hardness and opacity, and [ and ].

  return (
    <QueryClientProvider client={queryClient}>
      <I18nextProvider i18n={i18n}>
        <TooltipProvider delay={0}>
          <ClientOnly>
            <StartupBoundary>{children}</StartupBoundary>
            <Toaster />
          </ClientOnly>
        </TooltipProvider>
      </I18nextProvider>
    </QueryClientProvider>
  )
}

function StartupBoundary({ children }: { children: ReactNode }) {
  const initialized = useKoharuStore((state) => state.initialized)
  // Booharu: never mounts Koharu's updater, which would offer Koharu's releases instead of its own.
  return initialized ? <>{children}</> : <StartupView />
}

export default Providers
