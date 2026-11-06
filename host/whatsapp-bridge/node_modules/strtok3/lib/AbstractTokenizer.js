import { throwIfAborted } from './AbortSignal.js';
import { BufferSizeError, getMaxBufferSize } from './BufferSizeError.js';
import { EndOfStreamError } from './stream/index.js';
/**
 * Core tokenizer
 */
export class AbstractTokenizer {
    /**
     * Constructor
     * @param options Tokenizer options
     * @protected
     */
    constructor(options) {
        this.numBuffer = new Uint8Array(10); // Includes the 80-bit floating-point tokens.
        /**
         * Tokenizer-stream position
         */
        this.position = 0;
        this.maxBufferSize = getMaxBufferSize(options);
        throwIfAborted(options?.abortSignal);
        this.onClose = options?.onClose;
        if (options?.abortSignal) {
            this.addAbortListener(options.abortSignal, () => {
                void this.abort().catch(() => {
                    // Signal events cannot propagate cancellation errors; explicit abort/close calls still reject.
                });
            });
        }
    }
    addAbortListener(signal, listener) {
        this.abortSignal = signal;
        this.abortListener = listener;
        signal.addEventListener('abort', listener, { once: true });
    }
    removeAbortListener() {
        if (this.abortListener) {
            this.abortSignal?.removeEventListener('abort', this.abortListener);
        }
        this.abortSignal = undefined;
        this.abortListener = undefined;
    }
    /**
     * Read a token from the tokenizer-stream
     * @param token - The token to read
     * @param position - If provided, the desired position in the tokenizer-stream
     * @returns Promise with token data
     */
    async readToken(token, position = this.position) {
        this.validateTokenRead(token.len, position);
        const uint8Array = new Uint8Array(token.len);
        const len = await this.readBuffer(uint8Array, { position });
        if (len < token.len) {
            throw new EndOfStreamError();
        }
        return token.get(uint8Array, 0);
    }
    /**
     * Peek a token from the tokenizer-stream.
     * @param token - Token to peek from the tokenizer-stream.
     * @param position - Offset where to begin reading within the file. If position is null, data will be read from the current file position.
     * @returns Promise with token data
     */
    async peekToken(token, position = this.position) {
        const peekPosition = position ?? this.position;
        this.validateTokenRead(token.len, peekPosition, true);
        const uint8Array = new Uint8Array(token.len);
        const len = await this.peekBuffer(uint8Array, { position: peekPosition });
        if (len < token.len) {
            throw new EndOfStreamError();
        }
        return token.get(uint8Array, 0);
    }
    /**
     * Read a numeric token from the stream
     * @param token - Numeric token
     * @returns Promise with number
     */
    async readNumber(token) {
        const len = await this.readBuffer(this.numBuffer, { length: token.len });
        if (len < token.len) {
            throw new EndOfStreamError();
        }
        return token.get(this.numBuffer, 0);
    }
    /**
     * Read a numeric token from the stream
     * @param token - Numeric token
     * @returns Promise with number
     */
    async peekNumber(token) {
        const len = await this.peekBuffer(this.numBuffer, { length: token.len });
        if (len < token.len) {
            throw new EndOfStreamError();
        }
        return token.get(this.numBuffer, 0);
    }
    /**
     * Ignore number of bytes, advances the pointer in under tokenizer-stream.
     * @param length - Number of bytes to ignore.  Must be ≥ 0.
     * @return resolves the number of bytes ignored, equals length if this available, otherwise the number of bytes available
     */
    async ignore(length) {
        if (length < 0) {
            throw new RangeError('ignore length must be ≥ 0 bytes');
        }
        this.validateLength(length, 'ignore length');
        this.validateLength(this.position + length, 'end position');
        if (this.fileInfo.size !== undefined) {
            const bytesLeft = Math.max(0, this.fileInfo.size - this.position);
            if (length > bytesLeft) {
                this.position += bytesLeft;
                return bytesLeft;
            }
        }
        this.position += length;
        return length;
    }
    close() {
        if (!this.closePromise) {
            this.removeAbortListener();
            // Cache completion before invoking cleanup hooks or caller callbacks.
            this.closePromise = Promise.resolve().then(() => this.closeOnce());
        }
        return this.closePromise;
    }
    /** Perform cleanup once, sharing completion and the first error across close calls. */
    async closeOnce() {
        await this.abort();
        await this.onClose?.();
    }
    /**
     * Validate a byte length or position before using it in buffer calculations.
     * @param value - Value that must be a non-negative safe integer
     * @param name - Label used in the validation error
     * @throws RangeError if the value is negative or not a safe integer
     */
    validateLength(value, name) {
        if (!Number.isSafeInteger(value) || value < 0) {
            throw new RangeError(`${name} must be a non-negative safe integer`);
        }
    }
    /**
     * Validate a requested buffer size against the configured allocation limit.
     * @param length - Requested buffer size in bytes
     * @throws RangeError if the length is negative or not a safe integer
     * @throws BufferSizeError if the length exceeds maxBufferSize
     */
    validateBufferSize(length) {
        this.validateLength(length, 'buffer length');
        if (length > this.maxBufferSize) {
            throw new BufferSizeError(length, this.maxBufferSize);
        }
    }
    /**
     * Validate a token request before allocating a buffer or consuming input.
     * @param length - Token length in bytes
     * @param position - Byte offset where the token starts
     * @param peek - Whether to include skipped stream bytes in the lookahead limit
     * @throws RangeError if a length or position is negative or not a safe integer
     * @throws BufferSizeError if the token buffer or stream lookahead exceeds maxBufferSize
     * @throws Error if a stream request starts before the current position
     * @throws EndOfStreamError if the token extends beyond the known input size
     */
    validateTokenRead(length, position, peek = false) {
        this.validateBufferSize(length);
        this.validateLength(position, 'position');
        this.validateLength(position + length, 'end position');
        if (!this.supportsRandomAccess() && position < this.position) {
            throw new Error('`options.position` must be equal or greater than `tokenizer.position`');
        }
        if (peek && !this.supportsRandomAccess()) {
            this.validateBufferSize(length + position - this.position);
        }
        if (this.fileInfo.size !== undefined && length > this.fileInfo.size - position) {
            throw new EndOfStreamError();
        }
    }
    /**
     * Apply defaults and validate buffer read or peek options.
     * @param uint8Array - Target buffer whose length is the default read length
     * @param options - Requested length, position and partial-read behavior
     * @returns Validated options with an explicit length, position and mayBeLess flag
     * @throws RangeError if a length or position is invalid, or the length exceeds the target buffer
     * @throws Error if a stream request starts before the current position
     */
    normalizeOptions(uint8Array, options) {
        const length = options?.length ?? uint8Array.length;
        const position = options?.position ?? this.position;
        this.validateLength(length, 'length');
        this.validateLength(position, 'position');
        this.validateLength(position + length, 'end position');
        if (length > uint8Array.length) {
            throw new RangeError('length must not exceed the target buffer length');
        }
        if (!this.supportsRandomAccess() && position < this.position) {
            throw new Error('`options.position` must be equal or greater than `tokenizer.position`');
        }
        return { length, position, mayBeLess: options?.mayBeLess ?? false };
    }
    abort() {
        this.removeAbortListener();
        return Promise.resolve(); // Ignore abort signal
    }
}
