import { stat as fsStat } from 'node:fs/promises';
import { fromStream as coreFromStream } from './core.js';
import { FileTokenizer } from './FileTokenizer.js';
export * from './core.js';
export { FileTokenizer } from './FileTokenizer.js';
export { NotRegularFileError } from './NotRegularFileError.js';
async function waitForPendingOpen(stream) {
    if (stream.pending !== true || stream.destroyed || stream.errored !== null) {
        return;
    }
    await new Promise(resolve => {
        const settle = () => {
            stream.removeListener('open', settle);
            stream.removeListener('error', settle);
            stream.removeListener('close', settle);
            resolve();
        };
        stream.once('open', settle);
        stream.once('error', settle);
        stream.once('close', settle);
    });
}
/**
 * Construct ReadStreamTokenizer from given Stream.
 * Will set fileSize, if provided given Stream has set the .path property.
 * @param stream - Node.js Stream.Readable
 * @param options - Pass additional file information to the tokenizer
 * @returns Tokenizer
 */
export async function fromStream(stream, options) {
    const rst = coreFromStream(stream, options);
    try {
        if (stream.path) {
            const stat = await fsStat(stream.path);
            rst.fileInfo.path = stream.path;
            rst.fileInfo.size = stat.size;
        }
        return rst;
    }
    catch (error) {
        await waitForPendingOpen(stream);
        await rst.close();
        throw error;
    }
}
export const fromFile = FileTokenizer.fromFile;
