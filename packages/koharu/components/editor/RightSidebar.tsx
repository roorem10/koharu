'use client'

import { BookOpenText, Bot, Layers3 } from 'lucide-react'
import { useHotkeys } from 'react-hotkeys-hook'
import { useTranslation } from 'react-i18next'

import { ContextPanel, ContextTabLabel } from '@/components/booharu/context/ContextPanel' // Booharu: the Context tab.
import { QcTab, QcTabLabel } from '@/components/booharu/qc/QcTab' // Booharu: the QC tab.
import { TabTooltip } from '@/components/booharu/sidebar/TabTooltip' // Booharu: a tab named in a tooltip.
import { AgentPanel } from '@/components/editor/AgentPanel'
import { LayersPanel } from '@/components/editor/LayersPanel'
import { StoryPanel } from '@/components/editor/StoryPanel'
import { useQcOpenCount } from '@/lib/booharu/qc' // Booharu: the QC tab shows while it lists a finding.
import { useSidebarTabsFit } from '@/lib/booharu/sidebarTabs' // Booharu: five tabs in a narrow pane.
import { shortcutLabel, shortcuts } from '@/lib/shortcuts'
import { useKoharuStore, type Panel } from '@/lib/store'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@koharu/ui/components/tabs'

// Booharu: no list of tabs; the five are drawn one by one below, each by `named`.

/** The right pane: Layers, Story, and Agent as tabs, each its full height, as Photoshop groups its
 * panels. The agent's tab stays mounted, so a switch leaves its transcript where it was. */
export function RightSidebar() {
  const { t } = useTranslation()
  const panel = useKoharuStore((state) => state.panel)
  const showPanel = useKoharuStore((state) => state.showPanel)
  // Photoshop's key for Layers, which shows Story in its place when it's in front. It works from a
  // field too, where it types nothing: an input method's own F7 doesn't come as that key.
  const next = panel === 'layers' ? 'story' : 'layers'
  useHotkeys(shortcuts.layers, () => showPanel(next), {
    preventDefault: true,
    useKey: true,
    enableOnFormTags: true,
  })
  const findings = useQcOpenCount() // Booharu: the QC tab's count of open findings.
  const fit = useSidebarTabsFit() // Booharu: the tabs give way as they stop fitting whole.
  // Booharu: the tab `value` drawn named `name` in a tooltip while the tabs show their icons alone,
  // and while F7 shows it, with that key beside its name, as Koharu's tooltip names it.
  const named = (value: Panel, name: string) => (
    <TabTooltip
      name={name}
      alone={fit['data-booharu-tabs'] === 'icons'}
      shortcut={value === next ? shortcutLabel(shortcuts.layers) : undefined}
    />
  )
  // The shown tab in the accent colour, hovered or not, its underline too. Booharu: a tab shrinks,
  // never growing, its label cut short, and follows how the tabs fit: its icon goes while the whole
  // labels don't fit beside them, its padding narrows while they fit only tight, and its label is
  // left to screen readers and a tooltip, the icon showing alone, once even that's too wide.
  const tab =
    'h-7 flex-initial min-w-0 gap-1 px-1.5 text-[10px] font-semibold after:bg-primary data-active:text-primary data-active:hover:text-primary [&_svg]:size-3 group-data-[booharu-tabs=labels]/tabs-list:[&>svg]:hidden group-data-[booharu-tabs=tight]/tabs-list:[&>svg]:hidden group-data-[booharu-tabs=tight]/tabs-list:px-1 group-data-[booharu-tabs=icons]/tabs-list:[&>svg+span]:sr-only'
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
          // Booharu: the tabs closer together while they fit only tight.
          className='h-10 w-full shrink-0 justify-start rounded-none border-b border-border/80 px-1 data-[booharu-tabs=tight]:gap-0.5'
        >
          {/* Booharu: each label in a span cut short with an ellipsis, drawn by `named`. */}
          <TabsTrigger value='layers' className={tab} render={named('layers', t('layers.title'))}>
            <Layers3 /> <span className='truncate'>{t('layers.title')}</span>
          </TabsTrigger>
          <TabsTrigger value='story' className={tab} render={named('story', t('story.title'))}>
            <BookOpenText /> <span className='truncate'>{t('story.title')}</span>
          </TabsTrigger>
          {/* Booharu: the Context tab, the chapter's series and glossary, between Story and Agent. */}
          <TabsTrigger
            value='context'
            className={tab}
            render={named('context', t('booharu.context.title'))}
          >
            <ContextTabLabel />
          </TabsTrigger>
          <TabsTrigger value='agent' className={tab} render={named('agent', t('agent.title'))}>
            <Bot /> <span className='truncate'>{t('agent.title')}</span>
          </TabsTrigger>
          {/* Booharu: the QC tab, last, while it lists a finding, with their count. */}
          {findings > 0 && (
            <TabsTrigger value='qc' className={tab} render={named('qc', t('booharu.qc.title'))}>
              <QcTabLabel count={findings} />
            </TabsTrigger>
          )}
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
          <AgentPanel />
        </TabsContent>
        {/* Booharu: the QC tab's list of the boxes that need a look. */}
        <TabsContent value='qc' className='flex min-h-0 flex-col'>
          <QcTab />
        </TabsContent>
      </Tabs>
    </aside>
  )
}
