import { throwIfAborted } from './AbortSignal.js';
import { BlobTokenizer } from './BlobTokenizer.js';
import { getMaxBufferSize } from './BufferSizeError.js';
import { BufferTokenizer } from './BufferTokenizer.js';
import { ReadStreamTokenizer } from './ReadStreamTokenizer.js';
import { makeWebStreamReader, StreamReader } from './stream/index.js';
export { AbstractTokenizer } from './AbstractTokenizer.js';
export { BufferSizeError } from './BufferSizeError.js';
export { isRandomAccessTokenizer } from './isRandomAccessTokenizer.js';
export { AbortError, EndOfStreamError } from './stream/index.js';
export { TokenizerBoundaryError } from './TokenizerBoundaryError.js';
export { createTokenizerView } from './TokenizerView.js';
export { TokenWriter } from './TokenWriter.js';
/**
 * Construct ReadStreamTokenizer from given Stream.
 * Will set fileSize, if provided given Stream has set the .path property/
 * @param stream - Read from Node.js Stream.Readable
 * @param options - Tokenizer options
 * @returns ReadStreamTokenizer
 */
export function fromStream(stream, options) {
    getMaxBufferSize(options);
    throwIfAborted(options?.abortSignal);
    const streamReader = new StreamReader(stream);
    return new ReadStreamTokenizer(streamReader, options);
}
/**
 * Construct ReadStreamTokenizer from given ReadableStream (WebStream API).
 * Will set fileSize, if provided given Stream has set the .path property/
 * @param webStream - Read from Node.js Stream.Readable (must be a byte stream)
 * @param options - Tokenizer options
 * @returns ReadStreamTokenizer
 */
export function fromWebStream(webStream, options) {
    getMaxBufferSize(options);
    throwIfAborted(options?.abortSignal);
    const webStreamReader = makeWebStreamReader(webStream);
    return new ReadStreamTokenizer(webStreamReader, options);
}
/**
 * Construct ReadStreamTokenizer from given Buffer.
 * @param uint8Array - Uint8Array to tokenize
 * @param options - Tokenizer options
 * @returns BufferTokenizer
 */
export function fromBuffer(uint8Array, options) {
    return new BufferTokenizer(uint8Array, options);
}
/**
 * Construct ReadStreamTokenizer from given Blob.
 * @param blob - Uint8Array to tokenize
 * @param options - Tokenizer options
 * @returns BufferTokenizer
 */
export function fromBlob(blob, options) {
    return new BlobTokenizer(blob, options);
}
