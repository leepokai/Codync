/** Thrown when regularFileOnly is enabled and the opened handle is not a regular file. */
export class NotRegularFileError extends Error {
    constructor(path) {
        super(`Not a regular file: ${path}`);
        this.path = path;
        this.name = 'NotRegularFileError';
    }
}
