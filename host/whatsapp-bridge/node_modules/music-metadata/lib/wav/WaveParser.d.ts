import { BasicParser } from '../common/BasicParser.js';
import * as riff from '../riff/RiffChunk.js';
/**
 * Resource Interchange File Format (RIFF) Parser
 *
 * WAVE PCM soundfile format
 *
 * Ref:
 * - http://www.johnloomis.org/cpe102/asgn/asgn1/riff.html
 * - http://soundfile.sapp.org/doc/WaveFormat
 *
 * ToDo: Split WAVE part from RIFF parser
 */
export declare class WaveParser extends BasicParser {
    private codePage;
    private readonly pendingInfoTags;
    private fact;
    private blockAlign;
    private avgBytesPerSec;
    private header;
    parse(): Promise<void>;
    parseRiffChunk(chunkSize: number): Promise<void>;
    readWaveChunk(remaining: number): Promise<void>;
    parseListTag(listHeader: riff.IChunkHeader): Promise<void>;
    private parseRiffInfoTags;
    private addInfoTag;
    private flushInfoTags;
    private getInfoEncoding;
}
