import type { IRandomAccessTokenizer, ITokenizer } from './types.js';
/**
 * Check for random-access capability, position control and a known file size.
 * A known size alone does not imply random access.
 */
export declare function isRandomAccessTokenizer(tokenizer: ITokenizer): tokenizer is IRandomAccessTokenizer;
