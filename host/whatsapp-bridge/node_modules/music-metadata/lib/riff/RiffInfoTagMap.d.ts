import { CommonTagMapper } from '../common/GenericTagMapper.js';
import type { INativeTagMap } from '../common/GenericTagTypes.js';
/**
 * RIFF Info Tags; part of the EXIF 2.3
 * Ref: http://owl.phy.queensu.ca/~phil/exiftool/TagNames/RIFF.html#Info
 */
export declare const riffInfoTagMap: INativeTagMap;
export declare class RiffInfoTagMapper extends CommonTagMapper {
    constructor();
}
