import { isRandomAccessTokenizer } from './isRandomAccessTokenizer.js';
import { EndOfStreamError } from './stream/index.js';
import { TokenizerBoundaryError } from './TokenizerBoundaryError.js';
function validateOffset(value, name) {
    if (!Number.isSafeInteger(value) || value < 0) {
        throw new RangeError(`${name} must be a non-negative safe integer`);
    }
}
export function createTokenizerView(source, length) {
    return isRandomAccessTokenizer(source)
        ? new RandomAccessTokenizerView(source, length)
        : new TokenizerView(source, length);
}
class TokenizerView {
    constructor(source, size) {
        this.source = source;
        this.size = size;
        this.closed = false;
        this.closedPosition = 0;
        this.busy = false;
        validateOffset(size, 'length');
        validateOffset(source.position, 'source position');
        validateOffset(source.position + size, 'end position');
        if (source instanceof TokenizerView) {
            source.assertAvailable();
            source.translateRange(size, source.position);
        }
        this.start = source.position;
        // A chunk has its own size; the outer file's MIME type/path need not describe it.
        this.fileInfo = { size };
    }
    get position() {
        return this.closed ? this.closedPosition : this.source.position - this.start;
    }
    get remaining() {
        return this.size - this.position;
    }
    supportsRandomAccess() {
        return false;
    }
    createView(length) {
        this.assertAvailable();
        return createTokenizerView(this, length);
    }
    async readBuffer(buffer, options) {
        return this.bufferOperation('readBuffer', buffer, options);
    }
    async peekBuffer(buffer, options) {
        return this.bufferOperation('peekBuffer', buffer, options);
    }
    async bufferOperation(method, buffer, options) {
        this.assertAvailable();
        const length = options?.length ?? buffer.length;
        const position = options?.position ?? this.position;
        const absolutePosition = this.translateRange(length, position);
        if (length > buffer.length) {
            throw new RangeError('length must not exceed the target buffer length');
        }
        return this.run(() => this.source[method](buffer, { ...options, length, position: absolutePosition }));
    }
    async readToken(token, position = this.position) {
        this.assertAvailable();
        const absolutePosition = this.translateRange(token.len, position);
        return this.run(() => this.source.readToken(token, absolutePosition));
    }
    async peekToken(token, position = this.position) {
        this.assertAvailable();
        const absolutePosition = this.translateRange(token.len, position ?? this.position);
        return this.run(() => this.source.peekToken(token, absolutePosition));
    }
    async readNumber(token) {
        this.assertAvailable();
        this.translateRange(token.len, this.position);
        return this.run(() => this.source.readNumber(token));
    }
    async peekNumber(token) {
        this.assertAvailable();
        this.translateRange(token.len, this.position);
        return this.run(() => this.source.peekNumber(token));
    }
    async ignore(length) {
        this.assertAvailable();
        this.translateRange(length, this.position);
        return this.run(() => this.source.ignore(length));
    }
    async consumeRemainder() {
        this.assertAvailable();
        const length = this.remaining;
        const end = this.translateRange(length, this.position) + length;
        return this.run(async () => {
            if (length === 0) {
                return 0;
            }
            if (isRandomAccessTokenizer(this.source)) {
                // Unlike ignore()/setPosition(), this verifies that the final byte exists,
                // including a file that was truncated after its initial stat. No large read.
                if ((await this.source.peekBuffer(new Uint8Array(1), { position: end - 1 })) !== 1) {
                    throw new EndOfStreamError();
                }
                this.source.setPosition(end);
            }
            else {
                const ignored = await this.source.ignore(length);
                if (ignored !== length) {
                    throw new EndOfStreamError();
                }
            }
            return length;
        });
    }
    /** Release this view only. Never drain, close or abort the borrowed source. */
    async close() {
        if (this.closed) {
            return;
        }
        if (this.busy) {
            throw new Error('Cannot close a tokenizer view during an active operation');
        }
        this.closedPosition = this.position;
        this.closed = true;
    }
    /** Explicit cancellation propagates to the shared source and affects its other users. */
    async abort() {
        if (this.closed) {
            return;
        }
        try {
            await this.source.abort();
        }
        finally {
            this.closedPosition = this.position;
            this.closed = true;
        }
    }
    assertAvailable() {
        if (this.closed) {
            throw new Error('Tokenizer view is closed');
        }
        if (this.busy) {
            throw new Error('Tokenizer views require sequential operations');
        }
        if (this.source instanceof TokenizerView) {
            this.source.assertAvailable();
        }
    }
    /** Validate the whole request before delegating or allowing a token allocation. */
    translateRange(length, position) {
        validateOffset(length, 'length');
        validateOffset(position, 'position');
        validateOffset(position + length, 'end position');
        if (position > this.size || length > this.size - position) {
            throw new TokenizerBoundaryError(position, length, this.size);
        }
        if (!this.supportsRandomAccess() && position < this.position) {
            throw new Error('`options.position` must be equal or greater than `tokenizer.position`');
        }
        return this.start + position;
    }
    async run(operation) {
        this.busy = true;
        try {
            return await operation();
        }
        finally {
            this.busy = false;
        }
    }
}
class RandomAccessTokenizerView extends TokenizerView {
    constructor(source, size) {
        super(source, size);
    }
    supportsRandomAccess() {
        return true;
    }
    setPosition(position) {
        this.assertAvailable();
        this.source.setPosition(this.translateRange(0, position));
    }
    createView(length) {
        this.assertAvailable();
        return createTokenizerView(this, length);
    }
}
