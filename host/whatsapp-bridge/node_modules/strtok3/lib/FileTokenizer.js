import { constants } from 'node:fs';
import fs, {} from 'node:fs/promises';
import { throwIfAborted } from './AbortSignal.js';
import { AbstractTokenizer } from './AbstractTokenizer.js';
import { getMaxBufferSize } from './BufferSizeError.js';
import { NotRegularFileError } from './NotRegularFileError.js';
import { EndOfStreamError } from './stream/index.js';
export class FileTokenizer extends AbstractTokenizer {
    /**
     * Create tokenizer from provided file path
     * @param sourceFilePath File path
     * @param options Tokenizer options
     */
    static async fromFile(sourceFilePath, options) {
        getMaxBufferSize(options);
        throwIfAborted(options?.abortSignal);
        const flags = options?.regularFileOnly ? constants.O_RDONLY | constants.O_NONBLOCK : 'r';
        const fileHandle = await fs.open(sourceFilePath, flags);
        try {
            const stat = await fileHandle.stat();
            if (options?.regularFileOnly && !stat.isFile()) {
                throw new NotRegularFileError(sourceFilePath);
            }
            return new FileTokenizer(fileHandle, {
                ...options,
                fileInfo: { ...options?.fileInfo, path: sourceFilePath, size: stat.size }
            });
        }
        catch (error) {
            await fileHandle.close();
            throw error;
        }
    }
    constructor(fileHandle, options) {
        super(options);
        this.fileHandle = fileHandle;
        this.fileInfo = options.fileInfo;
    }
    /**
     * Read buffer from file
     * @param uint8Array - Uint8Array to write result to
     * @param options - Read behaviour options
     * @returns Promise number of bytes read
     */
    async readBuffer(uint8Array, options) {
        const normOptions = this.normalizeOptions(uint8Array, options);
        this.position = normOptions.position;
        if (normOptions.length === 0) {
            return 0;
        }
        const res = await this.fileHandle.read(uint8Array, 0, normOptions.length, normOptions.position);
        this.position += res.bytesRead;
        if (res.bytesRead < normOptions.length && (!options || !options.mayBeLess)) {
            throw new EndOfStreamError();
        }
        return res.bytesRead;
    }
    /**
     * Peek buffer from file
     * @param uint8Array - Uint8Array (or Buffer) to write data to
     * @param options - Read behaviour options
     * @returns Promise number of bytes read
     */
    async peekBuffer(uint8Array, options) {
        const normOptions = this.normalizeOptions(uint8Array, options);
        const res = await this.fileHandle.read(uint8Array, 0, normOptions.length, normOptions.position);
        if (!normOptions.mayBeLess && res.bytesRead < normOptions.length) {
            throw new EndOfStreamError();
        }
        return res.bytesRead;
    }
    async closeOnce() {
        await this.fileHandle.close();
        await super.closeOnce();
    }
    setPosition(position) {
        this.validateLength(position, 'position');
        this.position = position;
    }
    supportsRandomAccess() {
        return true;
    }
}
