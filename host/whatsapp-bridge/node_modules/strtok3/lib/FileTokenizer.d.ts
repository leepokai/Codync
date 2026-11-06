import { type FileHandle } from 'node:fs/promises';
import { AbstractTokenizer } from './AbstractTokenizer.js';
import type { IRandomAccessFileInfo, IRandomAccessTokenizer, IReadChunkOptions, ITokenizerOptions } from './types.js';
/** Options for creating a tokenizer from a file path. */
export interface IFromFileOptions extends ITokenizerOptions {
    /**
     * Open without blocking on FIFOs and require the opened handle to refer to a regular file.
     * Rejects with NotRegularFileError for an opened non-regular file and closes its handle.
     * Disabled by default, preserving the existing file-opening behavior.
     */
    regularFileOnly?: boolean;
}
interface IFileTokenizerOptions extends ITokenizerOptions {
    /**
     * Pass additional file information to the tokenizer
     */
    fileInfo: IRandomAccessFileInfo;
}
export declare class FileTokenizer extends AbstractTokenizer implements IRandomAccessTokenizer {
    private fileHandle;
    fileInfo: IRandomAccessFileInfo;
    /**
     * Create tokenizer from provided file path
     * @param sourceFilePath File path
     * @param options Tokenizer options
     */
    static fromFile(sourceFilePath: string, options?: IFromFileOptions): Promise<FileTokenizer>;
    protected constructor(fileHandle: FileHandle, options: IFileTokenizerOptions);
    /**
     * Read buffer from file
     * @param uint8Array - Uint8Array to write result to
     * @param options - Read behaviour options
     * @returns Promise number of bytes read
     */
    readBuffer(uint8Array: Uint8Array, options?: IReadChunkOptions): Promise<number>;
    /**
     * Peek buffer from file
     * @param uint8Array - Uint8Array (or Buffer) to write data to
     * @param options - Read behaviour options
     * @returns Promise number of bytes read
     */
    peekBuffer(uint8Array: Uint8Array, options?: IReadChunkOptions): Promise<number>;
    protected closeOnce(): Promise<void>;
    setPosition(position: number): void;
    supportsRandomAccess(): boolean;
}
export {};
