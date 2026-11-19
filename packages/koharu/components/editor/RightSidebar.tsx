'use client'

import { BookOpenText, Bot, Layers3 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { ContextPanel, ContextTabLabel } from '@/components/booharu/context/ContextPanel' // Booharu: the Context tab.
import { AgentPanel } from '@/components/editor/AgentPanel'
import { LayersPanel } from '@/components/editor/LayersPanel'
import { StoryPanel } from '@/components/editor/StoryPanel'
import { useSidebarTabsFit } from '@/lib/booharu/sidebarTabs' // Booharu: five tabs in a narrow pane.
import { useProject } from '@/lib/queries'
import { useKoharuStore, type Panel } from '@/lib/store'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@koharu/ui/components/tabs'

/** The right pane: Layers, Story, and Agent as tabs, each its full height, as Photoshop groups its
 * panels. The agent's tab stays mounted, so its conversation outlives a switch, and starts over
 * with another project, as the desktop's conversation does. */
export function RightSidebar() {
  const { t } = useTranslation()
  const project = useProject().data?.name ?? null
  const panel = useKoharuStore((state) => state.panel)
  const showPanel = useKoharuStore((state) => state.showPanel)
  const fit = useSidebarTabsFit() // Booharu: icons give way to labels when the tabs don't fit.
  // The shown tab in the accent colour, hovered or not, its underline too. Booharu: a tab shrinks,
  // never growing, its label cut short, and loses its icon while the labels don't fit whole.
  const tab =
    'h-7 flex-initial min-w-0 gap-1 px-1.5 text-[10px] font-semibold after:bg-primary data-active:text-primary data-active:hover:text-primary [&_svg]:size-3 group-data-[booharu-icons=hidden]/tabs-list:[&>svg]:hidden'
  return (
    <aside className='flex h-full min-h-0 flex-col bg-[var(--surface-panel)]'>
      <Tabs
        value={panel}
        onValueChange={(next) => showPanel(next as Panel)}
        className='min-h-0 flex-1 gap-0'
      >
        <TabsList
          {...fit} // Booharu: measured to fit its tabs.
          variant='line'
          className='h-10 w-full shrink-0 justify-start rounded-none border-b border-border/80 px-1'
        >
          {/* Booharu: each label in a span cut short with an ellipsis, named whole in the title. */}
          <TabsTrigger value='layers' className={tab} title={t('layers.title')}>
            <Layers3 /> <span className='truncate'>{t('layers.title')}</span>
          </TabsTrigger>
          <TabsTrigger value='story' className={tab} title={t('story.title')}>
            <BookOpenText /> <span className='truncate'>{t('story.title')}</span>
          </TabsTrigger>
          {/* Booharu: the Context tab, the chapter's series and glossary, between Story and Agent. */}
          <TabsTrigger value='context' className={tab} title={t('booharu.context.title')}>
            <ContextTabLabel />
          </TabsTrigger>
          <TabsTrigger value='agent' className={tab} title={t('agent.title')}>
            <Bot /> <span className='truncate'>{t('agent.title')}</span>
          </TabsTrigger>
        </TabsList>
        <TabsContent value='layers' className='flex min-h-0 flex-col'>
          <LayersPanel />
        </TabsContent>
        <TabsContent value='story' className='flex min-h-0 flex-col'>
          <StoryPanel />
        </TabsContent>
        {/* Booharu: the Context tab's panel. */}
        <TabsContent value='context' className='flex min-h-0 flex-col'>
          <ContextPanel />
        </TabsContent>
        <TabsContent value='agent' keepMounted className='flex min-h-0 flex-col'>
          <AgentPanel key={project} />
        </TabsContent>
      </Tabs>
    </aside>
  )
}
