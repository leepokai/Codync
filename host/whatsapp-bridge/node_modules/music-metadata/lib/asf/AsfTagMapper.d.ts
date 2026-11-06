import { CommonTagMapper } from '../common/GenericTagMapper.js';
import type { IWarningCollector } from '../common/MetadataCollector.js';
import type { IRating, ITag } from '../type.js';
export declare class AsfTagMapper extends CommonTagMapper {
    static toRating(rating: string | number): IRating;
    constructor();
    protected postMap(tag: ITag, warnings: IWarningCollector): void;
}
