/** A request exceeds a tokenizer view, rather than encountering truncated input. */
export declare class TokenizerBoundaryError extends RangeError {
    readonly position: number;
    readonly length: number;
    readonly size: number;
    constructor(position: number, length: number, size: number);
}
