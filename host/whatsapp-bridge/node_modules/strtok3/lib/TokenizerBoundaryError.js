/** A request exceeds a tokenizer view, rather than encountering truncated input. */
export class TokenizerBoundaryError extends RangeError {
    constructor(position, length, size) {
        super(`Range at position ${position} with length ${length} exceeds tokenizer view size ${size}`);
        this.position = position;
        this.length = length;
        this.size = size;
        this.name = 'TokenizerBoundaryError';
    }
}
