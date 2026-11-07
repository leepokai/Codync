import { Observable } from '../../lib/observable.ts'

/**
 * The code a device sign-in (GitHub) asks for, shown before its page opens: one at a time, for
 * every computer. ChatWindow renders it as a dialog above whatever sheet started the sign-in.
 */
export class SignInCode extends Observable {
  code: string | null = null
  #reply: ((open: boolean) => void) | null = null

  /** Shows `code`; true once the user wants it copied and the page opened. */
  ask(code: string) {
    this.#settle(false)
    this.code = code
    this.changed()
    return new Promise<boolean>((resolve) => (this.#reply = resolve))
  }

  open() {
    this.#settle(true)
  }

  /** The dialog closes before it runs its action, so a close only counts as Cancel once that action had its turn. */
  close() {
    queueMicrotask(() => this.#settle(false))
  }

  #settle(open: boolean) {
    const reply = this.#reply
    if (!reply) return
    this.#reply = null
    this.code = null
    this.changed()
    reply(open)
  }
}

export const signInCode = new SignInCode()
