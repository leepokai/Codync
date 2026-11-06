/** Thrown when regularFileOnly is enabled and the opened handle is not a regular file. */
export declare class NotRegularFileError extends Error {
    readonly path: string;
    constructor(path: string);
}
