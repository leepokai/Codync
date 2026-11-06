import type { SupportedEncoding } from '@borewit/text-codec';
import type { IGetToken } from 'strtok3';
import type { IChunkHeader } from '../iff/index.js';
export type { IChunkHeader } from '../iff/index.js';
/**
 * Common RIFF chunk header
 */
export declare const Header: IGetToken<IChunkHeader>;
/**
 * Token to parse RIFF-INFO tag value
 */
export declare class ListInfoTagValue implements IGetToken<string> {
    private readonly encoding;
    len: number;
    private tagHeader;
    constructor(tagHeader: IChunkHeader, encoding?: SupportedEncoding);
    get(buf: Uint8Array, off: number): string;
}
