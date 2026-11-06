import type { Readable } from 'node:stream';
import { type ITokenizerOptions } from './core.js';
import { FileTokenizer } from './FileTokenizer.js';
import type { ReadStreamTokenizer } from './ReadStreamTokenizer.js';
export type { IGetToken, IToken } from '@tokenizer/token';
export * from './core.js';
export { FileTokenizer, type IFromFileOptions } from './FileTokenizer.js';
export { NotRegularFileError } from './NotRegularFileError.js';
/**
 * Construct ReadStreamTokenizer from given Stream.
 * Will set fileSize, if provided given Stream has set the .path property.
 * @param stream - Node.js Stream.Readable
 * @param options - Pass additional file information to the tokenizer
 * @returns Tokenizer
 */
export declare function fromStream(stream: Readable, options?: ITokenizerOptions): Promise<ReadStreamTokenizer>;
export declare const fromFile: typeof FileTokenizer.fromFile;
