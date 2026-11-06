import type { ITokenizerOptions } from './types.js';
/** Thrown before a tokenizer or writer buffer exceeds its configured limit. */
export declare class BufferSizeError extends RangeError {
    readonly requestedSize: number;
    readonly maxBufferSize: number;
    /**
     * Create an error describing an allocation that exceeds the configured limit.
     * @param requestedSize - Requested buffer size in bytes
     * @param maxBufferSize - Configured maximum buffer size in bytes
     */
    constructor(requestedSize: number, maxBufferSize: number);
}
/**
 * Validate options before acquiring file handles or stream readers.
 * @param options - Tokenizer options containing the optional allocation limit
 * @returns Configured buffer limit in bytes, or Infinity when the limit is disabled
 * @throws RangeError if maxBufferSize is neither a positive safe integer nor Infinity
 */
export declare function getMaxBufferSize(options?: ITokenizerOptions): number;
