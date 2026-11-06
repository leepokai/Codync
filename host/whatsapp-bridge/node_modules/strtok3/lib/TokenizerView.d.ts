import type { IRandomAccessTokenizer, IRandomAccessTokenizerView, ITokenizer, ITokenizerView } from './types.js';
/**
 * Borrow length bytes starting at source.position. Creation never consumes input.
 * All reads advance the shared source cursor; peeks leave it unchanged.
 * Use source, parent and child sequentially. Closing a partial child leaves the
 * parent at the consumed position; consumeRemainder() explicitly advances to its end.
 */
export declare function createTokenizerView(source: IRandomAccessTokenizer, length: number): IRandomAccessTokenizerView;
export declare function createTokenizerView(source: ITokenizer, length: number): ITokenizerView;
