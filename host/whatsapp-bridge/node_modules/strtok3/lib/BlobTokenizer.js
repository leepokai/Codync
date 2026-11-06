import { AbstractTokenizer } from './AbstractTokenizer.js';
import { EndOfStreamError } from './stream/index.js';
export class BlobTokenizer extends AbstractTokenizer {
    /**
     * Construct BufferTokenizer
     * @param blob - Uint8Array to tokenize
     * @param options Tokenizer options
     */
    constructor(blob, options) {
        super(options);
        this.blob = blob;
        this.fileInfo = { ...(options?.fileInfo ?? {}), ...{ size: blob.size, mimeType: blob.type } };
    }
    /**
     * Read buffer from tokenizer
     * @param uint8Array - Uint8Array to tokenize
     * @param options - Read behaviour options
     * @returns {Promise<number>}
     */
    async readBuffer(uint8Array, options) {
        const normOptions = this.normalizeOptions(uint8Array, options);
        const bytesRead = await this.peekBuffer(uint8Array, normOptions);
        this.position = normOptions.position + bytesRead;
        return bytesRead;
    }
    /**
     * Peek (read ahead) buffer from tokenizer
     * @param buffer
     * @param options - Read behaviour options
     * @returns {Promise<number>}
     */
    async peekBuffer(buffer, options) {
        const normOptions = this.normalizeOptions(buffer, options);
        const bytes2read = Math.max(0, Math.min(this.blob.size - normOptions.position, normOptions.length));
        if (!normOptions.mayBeLess && bytes2read < normOptions.length) {
            throw new EndOfStreamError();
        }
        for (let offset = 0; offset < bytes2read;) {
            const chunkLength = Math.min(this.maxBufferSize, bytes2read - offset);
            const position = normOptions.position + offset;
            const arrayBuffer = await this.blob.slice(position, position + chunkLength).arrayBuffer();
            buffer.set(new Uint8Array(arrayBuffer), offset);
            offset += chunkLength;
        }
        return bytes2read;
    }
    close() {
        return super.close();
    }
    supportsRandomAccess() {
        return true;
    }
    setPosition(position) {
        this.validateLength(position, 'position');
        this.position = position;
    }
}
