import { Component, type ReactNode } from 'react'
import { EmptyState } from './HostStateView'

/** A view that throws while rendering would unmount the whole window and leave it blank: this shows an error state with a reload instead. */
export class RenderBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }

  static getDerivedStateFromError() {
    return { failed: true }
  }

  render() {
    if (!this.state.failed) return this.props.children
    return <EmptyState icon="exclamationmark.triangle.fill" title="Something went wrong" action={['Try again', () => location.reload()]} />
  }
}
