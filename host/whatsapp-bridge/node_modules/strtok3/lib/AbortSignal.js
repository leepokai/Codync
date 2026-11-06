import { AbortError } from './stream/Errors.js';
/** Reject an already-cancelled operation before acquiring tokenizer resources. */
export function throwIfAborted(signal) {
    if (signal?.aborted) {
        throw new AbortError();
    }
}
