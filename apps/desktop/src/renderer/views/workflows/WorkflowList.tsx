import { Text } from '../../components/Text'
import { useModels } from '../../lib/observable'
import { prefs, toggled, usePref } from '../../lib/prefs'
import { useTr } from '../../lib/use-language'
import type { BotStore } from '../../store/bot-store'
import { WORK_MARK, WorkDot } from '../chat/WorkDialogs'
import { ComputerBadge } from '../ComputerBadge'
import { ListSectionHeading } from '../ListSectionHeading'
import { canOpenWorkflows, reachable } from './pins'
import type { RunStatus } from './workflow'
import '../threads-page.css'

/** Where the canvas is: which computer, and which of its workflows. */
export interface WorkflowPlace { computerId: string; workflowId: string | null }

/**
 * The Workflows page's list, like the Threads list: each computer shown, under its name, and its workflows below; a run
 * under way marks its row; a computer's heading folds its workflows away. A computer that is offline says so. New
 * workflows come from the header's `+`.
 */
export function WorkflowList({ stores, place, open }: {
  stores: BotStore[]
  place: WorkflowPlace | null
  open: (computerId: string, workflowId: string) => void
}) {
  const tr = useTr()
  useModels(stores)
  const [folded, setFolded] = usePref(prefs.foldedWorkflowComputers)
  // Every computer but one that is reachable and has no workflows (an older Codync).
  const listed = stores.filter((s) => canOpenWorkflows(s) || !reachable(s))
  return (
    <>
      {listed.map((store) => {
        const id = store.computer.id
        const ready = canOpenWorkflows(store)
        const isFolded = folded.includes(id)
        // Folded, its heading still says that a workflow there is running or waits for the user.
        const loud = isFolded ? (store.workflows.find((f) => f.running === 'approval') ?? store.workflows.find((f) => f.running)) : undefined
        return (
          <section key={id} className="thread-group" aria-label={store.hostName}>
            <ListSectionHeading
              leading={<ComputerBadge computer={store.computer} size={16} />}
              title={store.hostName}
              mark={loud?.running ? <span className="thread-mark"><RunMark status={loud.running} /></span> : null}
              folded={isFolded}
              onFold={() => setFolded(toggled(folded, id))}
            />
            <div className={`computer-roster-content ${isFolded ? 'collapsed' : ''}`} inert={isFolded} aria-hidden={isFolded}>
              <div className="thread-group-rows">
                {!ready ? (
                  <div className="thread-row"><Text tone="tertiary" truncate>{tr(store.connectionLabel)}</Text></div>
                ) : store.workflows.length === 0 ? (
                  <div className="thread-row"><Text tone="tertiary" truncate>{tr('No workflows yet')}</Text></div>
                ) : store.workflows.map((workflow) => (
                  <button
                    key={workflow.id}
                    type="button"
                    className={`thread-row ${place?.computerId === id && place.workflowId === workflow.id ? 'selected' : ''}`}
                    title={workflow.title}
                    onClick={() => open(id, workflow.id)}
                  >
                    <Text tone="text" truncate grow>{workflow.title}</Text>
                    {workflow.running ? <span className="thread-mark"><RunMark status={workflow.running} /></span> : null}
                  </button>
                ))}
              </div>
            </div>
          </section>
        )
      })}
    </>
  )
}

/** A run under way: waiting for an approval needs the user; otherwise it works. */
function RunMark({ status }: { status: RunStatus }) {
  if (status === 'approval') return <WorkDot mark={WORK_MARK.needsYou} />
  return status === 'running' || status === 'stopping' ? <WorkDot mark={WORK_MARK.working} /> : null
}
