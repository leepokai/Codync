import type { IGetToken, IToken } from '@tokenizer/token';
import type { IFileInfo, IReadChunkOptions, ITokenizer, ITokenizerOptions } from './types.js';
interface INormalizedReadChunkOptions extends IReadChunkOptions {
    length: number;
    position: number;
    mayBeLess?: boolean;
}
/**
 * Core tokenizer
 */
export declare abstract class AbstractTokenizer implements ITokenizer {
    private onClose?;
    private closePromise?;
    private abortSignal?;
    private abortListener?;
    private numBuffer;
    protected readonly maxBufferSize: number;
    abstract fileInfo: IFileInfo;
    /**
     * Tokenizer-stream position
     */
    position: number;
    /**
     * Constructor
     * @param options Tokenizer options
     * @protected
     */
    protected constructor(options?: ITokenizerOptions);
    private addAbortListener;
    protected removeAbortListener(): void;
    abstract supportsRandomAccess(): boolean;
    /**
     * Read buffer from tokenizer
     * @param buffer - Target buffer to fill with data read from the tokenizer-stream
     * @param options - Additional read options
     * @returns Promise with number of bytes read
     */
    abstract readBuffer(buffer: Uint8Array, options?: IReadChunkOptions): Promise<number>;
    /**
     * Peek (read ahead) buffer from tokenizer
     * @param uint8Array - Target buffer to fill with data peeked from the tokenizer-stream
     * @param options - Peek behaviour options
     * @returns Promise with number of bytes read
     */
    abstract peekBuffer(uint8Array: Uint8Array, options?: IReadChunkOptions): Promise<number>;
    /**
     * Read a token from the tokenizer-stream
     * @param token - The token to read
     * @param position - If provided, the desired position in the tokenizer-stream
     * @returns Promise with token data
     */
    readToken<Value>(token: IGetToken<Value>, position?: number): Promise<Value>;
    /**
     * Peek a token from the tokenizer-stream.
     * @param token - Token to peek from the tokenizer-stream.
     * @param position - Offset where to begin reading within the file. If position is null, data will be read from the current file position.
     * @returns Promise with token data
     */
    peekToken<Value>(token: IGetToken<Value>, position?: number | null): Promise<Value>;
    /**
     * Read a numeric token from the stream
     * @param token - Numeric token
     * @returns Promise with number
     */
    readNumber(token: IToken<number>): Promise<number>;
    /**
     * Read a numeric token from the stream
     * @param token - Numeric token
     * @returns Promise with number
     */
    peekNumber(token: IToken<number>): Promise<number>;
    /**
     * Ignore number of bytes, advances the pointer in under tokenizer-stream.
     * @param length - Number of bytes to ignore.  Must be ≥ 0.
     * @return resolves the number of bytes ignored, equals length if this available, otherwise the number of bytes available
     */
    ignore(length: number): Promise<number>;
    close(): Promise<void>;
    /** Perform cleanup once, sharing completion and the first error across close calls. */
    protected closeOnce(): Promise<void>;
    /**
     * Validate a byte length or position before using it in buffer calculations.
     * @param value - Value that must be a non-negative safe integer
     * @param name - Label used in the validation error
     * @throws RangeError if the value is negative or not a safe integer
     */
    protected validateLength(value: number, name: string): void;
    /**
     * Validate a requested buffer size against the configured allocation limit.
     * @param length - Requested buffer size in bytes
     * @throws RangeError if the length is negative or not a safe integer
     * @throws BufferSizeError if the length exceeds maxBufferSize
     */
    protected validateBufferSize(length: number): void;
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
    private validateTokenRead;
    /**
     * Apply defaults and validate buffer read or peek options.
     * @param uint8Array - Target buffer whose length is the default read length
     * @param options - Requested length, position and partial-read behavior
     * @returns Validated options with an explicit length, position and mayBeLess flag
     * @throws RangeError if a length or position is invalid, or the length exceeds the target buffer
     * @throws Error if a stream request starts before the current position
     */
    protected normalizeOptions(uint8Array: Uint8Array, options?: IReadChunkOptions): INormalizedReadChunkOptions;
    abort(): Promise<void>;
}
export {};
