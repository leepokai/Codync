import type { IToken } from '@tokenizer/token';
/** Configuration for the in-memory token writer. */
export interface ITokenWriterOptions {
    /**
     * Maximum output length and backing-buffer capacity in bytes.
     * Disabled by default (Infinity). Must be a positive safe integer or Infinity.
     * Does not limit total memory, input buffers, snapshots or encoder allocations.
     */
    maxBufferSize?: number;
}
/**
 * Growable byte buffer for serializing tokens.
 *
 * Numeric and structured tokens are written through their `IToken.put`
 * implementation, so encoding and decoding share the same token definition.
 * Encoders must write synchronously within their declared token region and
 * must not retain the destination buffer.
 */
export declare class TokenWriter {
    private buffer;
    private offset;
    private readonly maxBufferSize;
    /**
     * Create a token writer.
     * @param initialCapacity - Initial buffer size in bytes. The writer grows as needed.
     * Defaults to 1024, reduced to maxBufferSize when a smaller cap is configured.
     * @param options - Optional output and backing-buffer limit.
     */
    constructor(initialCapacity?: number, options?: ITokenWriterOptions);
    /**
     * Number of bytes written so far.
     */
    get length(): number;
    /**
     * Reserve a zero-filled region and advance the write position.
     * @param length - Number of bytes to reserve.
     * @returns Offset of the reserved region.
     */
    reserve(length: number): number;
    /**
     * Encode a token at the current write position.
     * If encoding throws, the written bytes and length remain unchanged.
     * @param token - Token providing the byte length and encoder.
     * @param value - Value to encode.
     * @returns Number of bytes written, equal to token.len.
     */
    writeToken<Value>(token: IToken<Value>, value: Value): number;
    /**
     * Append raw bytes.
     * @param value - Bytes to append. The data is copied.
     * @returns Number of bytes written.
     */
    writeBuffer(value: Uint8Array): number;
    /**
     * Alias for writeBuffer. The data is copied.
     * @param value - Bytes to append.
     * @returns Number of bytes written.
     */
    writeBytes(value: Uint8Array): number;
    /**
     * Append zero bytes.
     * @param length - Number of zero bytes to append.
     * @returns Number of bytes written.
     */
    writeZeros(length: number): number;
    /**
     * Overwrite a previously reserved or written region with an encoded token.
     * If encoding throws, the original bytes are restored and length is unchanged.
     * @param token - Token providing the byte length and encoder.
     * @param offset - Absolute offset of the region.
     * @param value - Value to encode.
     */
    patchToken<Value>(token: IToken<Value>, offset: number, value: Value): void;
    /**
     * Return a copy of the written bytes.
     */
    toUint8Array(): Uint8Array;
    private ensureCapacity;
    private validateCapacity;
    private validateLength;
}
