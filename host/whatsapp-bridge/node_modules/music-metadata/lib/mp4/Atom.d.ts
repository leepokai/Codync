import type { ITokenizer } from 'strtok3';
import * as AtomToken from './AtomToken.js';
export type AtomDataHandler = (atom: Atom, remaining: number) => Promise<void>;
export declare class Atom {
    private readonly payloadLength;
    static readAtom(tokenizer: ITokenizer, dataHandler: AtomDataHandler, parent: Atom | null, remaining: number): Promise<Atom>;
    readonly children: Atom[];
    readonly atomPath: string;
    readonly header: AtomToken.IAtomHeader;
    extended: boolean;
    readonly parent: Atom | null;
    constructor(header: AtomToken.IAtomHeader, extended: boolean, parent: Atom | null, payloadLength: number);
    getHeaderLength(): number;
    getPayloadLength(): number;
    readAtoms(tokenizer: ITokenizer, dataHandler: AtomDataHandler, size: number): Promise<void>;
    private readData;
}
