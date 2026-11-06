import { useState } from 'react'
import { Button, IconButton, SegmentedChoice } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Text } from '../../components/Text'
import { accessPages, accessPageReady, type AccessPage } from '../../lib/access-onboarding'
import { useStarAsk } from '../../lib/use-star-ask'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import { AccessNotice } from './ComputerAccessView'
import { AccessSteps } from './AccessSteps'
import { useComputerAccess } from './use-computer-access'
import appIcon from '../../../../resources/icon.png'
import './access-onboarding.css'

export function AccessOnboarding({ onContinue }: { onContinue: (ready: boolean) => void }) {
  const access = useComputerAccess()
  const tr = useTr()
  const star = useStarAsk()
  const pages = accessPages(window.codync.platform)
  const [selection, setSelection] = useState<AccessPage>(pages[0]!.id)
  const index = pages.findIndex((page) => page.id === selection)
  const ready = access.step === 'ready'
  useScreenViewed('computer_access')

  const finish = () => {
    star.answer(false)
    onContinue(ready)
  }

  return <section className="computer-access access-onboarding" aria-label={tr('Computer access setup')}>
    <div className="access-onboarding-content">
      <header className="access-onboarding-header">
        <img src={appIcon} width={80} height={80} alt="Codync" draggable={false} />
        <Text as="div" size="body" tone="secondary">{tr('Computer access')}</Text>
        <Text as="div" size="caption" tone="secondary">{access.computerName}</Text>
      </header>
      <div className="access-onboarding-picker" role="group" aria-label={tr('Computer access setup')}>
        <SegmentedChoice selection={selection} options={pages} onChange={setSelection} />
      </div>
      <div className="access-onboarding-stage">
        {pages.map((page, i) => <div key={page.id}
          className={`access-onboarding-panel ${selection === page.id ? 'selected' : i < index ? 'before' : 'after'}`}
          inert={selection !== page.id} aria-hidden={selection !== page.id}>
          {page.id === 'star' ? <div className="access-onboarding-star">
            <div className="access-onboarding-star-icon"><Icon name="star" size={28} /></div>
            <h2>{tr('Star Codync on GitHub')}</h2>
            <p>{tr('Codync is free and open source. A star on GitHub is how other people find it.')}</p>
            <Button onClick={() => star.answer(true)}><Icon name="star" size={14} />{tr('Star on GitHub')}<Icon name="arrow.up.right.square" size={12} /></Button>
            <Text as="div" size="caption" tone="secondary">{tr(' (optional)').trim()}</Text>
          </div> : <>
            <AccessSteps access={access} selection={page.id} />
            {access.step === 'host' ? <AccessNotice title="Waiting for your computer" icon="desktopcomputer">Start or restart the host from Codync’s menu to continue. We’ll check again automatically.</AccessNotice> : null}
            {access.step === 'unavailable' ? <AccessNotice title="Codync Screen is missing" icon="desktopcomputer.trianglebadge.exclamationmark">Install the full Codync desktop app to finish setup.</AccessNotice> : null}
            {access.error ? <AccessNotice title="Couldn’t finish setup" icon="exclamationmark.circle" error>{access.error}</AccessNotice> : null}
            {['capture', 'input', 'portal'].includes(page.id) && ['enable', 'helper', 'background'].includes(access.step)
              ? <div className="access-onboarding-prerequisite"><Button kind="secondary" onClick={() => setSelection('helper')}>{tr('Enable Codync Screen')}</Button></div> : null}
          </>}
        </div>)}
      </div>
      <footer className="access-onboarding-footer">
        <Text size="caption" tone="secondary" tabular>{index + 1} / {pages.length}</Text>
        <div className="access-onboarding-navigation">
          {selection !== 'star' ? <IconButton title="Check again" icon="arrow.clockwise" onClick={() => void access.refresh()} /> : null}
          <Button kind="secondary" onClick={() => selection === 'star' ? finish() : setSelection(pages[index + 1]!.id)}>
            {tr(selection === 'star' ? 'Get started' : accessPageReady(selection, access.step, access.screen) ? 'Continue' : 'Set up later')}<Icon name="arrow.right" size={12} />
          </Button>
        </div>
      </footer>
      <p className="access-onboarding-note">{tr('Permissions stay on this computer. You can change them later in Settings → Computer access.')}</p>
    </div>
  </section>
}
