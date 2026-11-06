import { BufferSizeError, getMaxBufferSize } from './BufferSizeError.js';
const DEFAULT_INITIAL_CAPACITY = 1024;
const MINIMUM_CAPACITY = 16;
/**
 * Growable byte buffer for serializing tokens.
 *
 * Numeric and structured tokens are written through their `IToken.put`
 * implementation, so encoding and decoding share the same token definition.
 * Encoders must write synchronously within their declared token region and
 * must not retain the destination buffer.
 */
export class TokenWriter {
    /**
     * Create a token writer.
     * @param initialCapacity - Initial buffer size in bytes. The writer grows as needed.
     * Defaults to 1024, reduced to maxBufferSize when a smaller cap is configured.
     * @param options - Optional output and backing-buffer limit.
     */
    constructor(initialCapacity, options) {
        this.offset = 0;
        this.maxBufferSize = getMaxBufferSize(options);
        const capacity = initialCapacity === undefined ? Math.min(DEFAULT_INITIAL_CAPACITY, this.maxBufferSize) : initialCapacity;
        this.validateCapacity(capacity, 'initialCapacity');
        this.buffer = new Uint8Array(Math.min(this.maxBufferSize, Math.max(MINIMUM_CAPACITY, capacity)));
    }
    /**
     * Number of bytes written so far.
     */
    get length() {
        return this.offset;
    }
    /**
     * Reserve a zero-filled region and advance the write position.
     * @param length - Number of bytes to reserve.
     * @returns Offset of the reserved region.
     */
    reserve(length) {
        this.validateLength(length, 'length');
        const start = this.offset;
        this.ensureCapacity(length);
        this.offset += length;
        return start;
    }
    /**
     * Encode a token at the current write position.
     * If encoding throws, the written bytes and length remain unchanged.
     * @param token - Token providing the byte length and encoder.
     * @param value - Value to encode.
     * @returns Number of bytes written, equal to token.len.
     */
    writeToken(token, value) {
        const length = token.len;
        this.validateLength(length, 'token length');
        this.ensureCapacity(length);
        const start = this.offset;
        try {
            token.put(this.buffer, start, value);
        }
        catch (error) {
            this.buffer.fill(0, start, start + length);
            throw error;
        }
        this.offset += length;
        return length;
    }
    /**
     * Append raw bytes.
     * @param value - Bytes to append. The data is copied.
     * @returns Number of bytes written.
     */
    writeBuffer(value) {
        this.validateLength(value.length, 'value length');
        const start = this.offset;
        this.ensureCapacity(value.length);
        this.buffer.set(value, start);
        this.offset += value.length;
        return value.length;
    }
    /**
     * Alias for writeBuffer. The data is copied.
     * @param value - Bytes to append.
     * @returns Number of bytes written.
     */
    writeBytes(value) {
        return this.writeBuffer(value);
    }
    /**
     * Append zero bytes.
     * @param length - Number of zero bytes to append.
     * @returns Number of bytes written.
     */
    writeZeros(length) {
        const start = this.reserve(length);
        this.buffer.fill(0, start, start + length);
        return length;
    }
    /**
     * Overwrite a previously reserved or written region with an encoded token.
     * If encoding throws, the original bytes are restored and length is unchanged.
     * @param token - Token providing the byte length and encoder.
     * @param offset - Absolute offset of the region.
     * @param value - Value to encode.
     */
    patchToken(token, offset, value) {
        this.validateLength(offset, 'offset');
        const length = token.len;
        this.validateLength(length, 'token length');
        if (offset + length > this.offset) {
            throw new RangeError(`Invalid patch range: offset=${offset}, length=${length}, written=${this.offset}`);
        }
        const original = this.buffer.slice(offset, offset + length);
        try {
            token.put(this.buffer, offset, value);
        }
        catch (error) {
            this.buffer.set(original, offset);
            throw error;
        }
    }
    /**
     * Return a copy of the written bytes.
     */
    toUint8Array() {
        return this.buffer.slice(0, this.offset);
    }
    ensureCapacity(extra) {
        const required = this.offset + extra;
        this.validateCapacity(required, 'required capacity');
        if (required <= this.buffer.length) {
            return;
        }
        let capacity = this.buffer.length;
        while (capacity < required) {
            capacity = Math.min(capacity * 2, this.maxBufferSize);
        }
        const next = new Uint8Array(capacity);
        next.set(this.buffer.subarray(0, this.offset));
        this.buffer = next;
    }
    validateCapacity(value, name) {
        this.validateLength(value, name);
        if (value > this.maxBufferSize) {
            throw new BufferSizeError(value, this.maxBufferSize);
        }
    }
    validateLength(value, name) {
        if (!Number.isSafeInteger(value) || value < 0) {
            throw new RangeError(`${name} must be a non-negative safe integer`);
        }
    }
}
