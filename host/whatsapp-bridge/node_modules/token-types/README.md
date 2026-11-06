[![Node.js CI](https://github.com/Borewit/token-types/actions/workflows/nodejs-ci.yml/badge.svg?branch=master)](https://github.com/Borewit/token-types/actions/workflows/nodejs-ci.yml?query=branch%3Amaster)
[![NPM version](https://badge.fury.io/js/token-types.svg)](https://npmjs.org/package/token-types)
[![npm downloads](http://img.shields.io/npm/dm/token-types.svg)](https://npmcharts.com/compare/token-types,strtok3?start=1200&interval=30)
[![codecov](https://codecov.io/gh/Borewit/token-types/branch/master/graph/badge.svg)](https://codecov.io/gh/Borewit/token-types)
[![DeepScan grade](https://deepscan.io/api/teams/5165/projects/6940/branches/61852/badge/grade.svg)](https://deepscan.io/dashboard#view=project&tid=5165&pid=6940&bid=61852)
[![Known Vulnerabilities](https://snyk.io/test/github/Borewit/token-types/badge.svg?targetFile=package.json)](https://snyk.io/test/github/Borewit/token-types?targetFile=package.json)

# token-types

A primitive token library used to read and write from a `Uint8Array`.
Although it is possible to use this module directly, it is primarily designed to be used with [strtok3 tokenizer](https://github.com/Borewit/strtok3).

## Compatibility

Module: version 5 migrated from [CommonJS](https://en.wikipedia.org/wiki/CommonJS) to [pure ECMAScript Module (ESM)](https://gist.github.com/sindresorhus/a39789f98801d908bbc7ff3ecc99d99c).
JavaScript is compliant with [ECMAScript 2019 (ES10)](https://en.wikipedia.org/wiki/ECMAScript#10th_Edition_%E2%80%93_ECMAScript_2019).

Node.js 18 is supported for running the published package; it is not a supported
build environment. The test toolchain requires Node.js 22.12 or later. CI builds
on Node.js 24 and runs the tests on Node.js 22, 24 and 26.

## Development

Use Node.js 24 and the Yarn version pinned in `package.json` to install development
dependencies and build from source:

```sh
corepack enable
yarn install --immutable
yarn test
```

`yarn test` type-checks the sources and runs the Vitest suite against the
TypeScript sources. `yarn test:watch` runs Vitest in watch mode, and
`yarn test-coverage` writes text and LCOV reports to `coverage`. `yarn clean`
removes `dist` and coverage.

`yarn lint` checks the Markdown and Biome rules. `yarn format:check` verifies
Biome formatting, and `yarn fix` applies formatting, import organization and
safe lint fixes.

## Installation

```sh
npm install --save token-types
```
Usually in combination with [strtok3](https://github.com/Borewit/strtok3):
```sh
npm install --save strtok3
```

Using TypeScript you should also install [@tokenizer/token](https://github.com/Borewit/tokenizer-token) as a development
dependency:

```shell
npm install --save-dev @tokenizer/token
```


## Example

```js
import * as strtok3 from 'strtok3';
import * as token from 'token-types';

(async () => {

  const tokenizer = await strtok3.fromFile("somefile.bin");
  try {
    const myNumber = await tokenizer.readToken(token.Float32_BE);
    console.log(`My number: ${myNumber}`);
  } finally {
    tokenizer.close(); // Close the file
  }
})();
```

## Tokens

### Numeric tokens

`token-types` supports a wide variety of numeric tokens out of the box:

| Token         | Number           | Bits | Endianness     |
|---------------|------------------|------|----------------|
| `UINT8`       | Unsigned integer |    8 | n/a            |
| `UINT16_BE`   | Unsigned integer |   16 | big endian     |
| `UINT16_LE`   | Unsigned integer |   16 | little endian  |
| `UINT24_BE`   | Unsigned integer |   24 | big endian     |
| `UINT24_LE`   | Unsigned integer |   24 | little endian  |
| `UINT32_BE`   | Unsigned integer |   32 | big endian     |
| `UINT32_LE`   | Unsigned integer |   32 | little endian  |
| `UINT64_BE`   | Unsigned integer |   64 | big endian     |
| `UINT64_LE`*  | Unsigned integer |   64 | little endian  |
| `INT8`        | Signed integer   |    8 | n/a            |
| `INT16_BE`    | Signed integer   |   16 | big endian     |
| `INT16_LE`    | Signed integer   |   16 | little endian  |
| `INT24_BE`    | Signed integer   |   24 | big endian     |
| `INT24_LE`    | Signed integer   |   24 | little endian  |
| `INT32_BE`    | Signed integer   |   32 | big endian     |
| `INT32_LE`    | Signed integer   |   32 | little endian  |
| `INT64_BE`    | Signed integer   |   64 | big endian     |
| `INT64_LE`*   | Signed integer   |   64 | little endian  |
| `Float16_BE`  | IEEE 754 float   |   16 | big endian     |
| `Float16_LE`  | IEEE 754 float   |   16 | little endian  |
| `Float32_BE`  | IEEE 754 float   |   32 | big endian     |
| `Float32_LE`  | IEEE 754 float   |   32 | little endian  |
| `Float64_BE`  | IEEE 754 float   |   64 | big endian     |
| `Float64_LE`  | IEEE 754 float   |   64 | little endian  |
| `Float80_BE`* | IEEE 754 float   |   80 | big endian     |
| `Float80_LE`* | IEEE 754 float   |   80 | little endian  |

(*) The tokens exceed the JavaScript IEEE 754 64-bit Floating Point precision, decoding and encoding is best effort based.

### String and byte tokens

`StringType` decoding is using [@borewit/text-codec](https://github.com/Borewit/text-codec).

`StringType` and `Uint8ArrayType` implement `IToken`, so the same fixed-size
token can decode and encode:

```js
import { StringType, Uint8ArrayType } from 'token-types';

const ascii = new StringType(4, 'ascii');
const buffer = new Uint8Array(8);
ascii.put(buffer, 0, 'fLaC');

const bytes = new Uint8ArrayType(4);
bytes.put(buffer, 4, Uint8Array.from([1, 2, 3, 4]));
```

`put` rejects values whose byte length differs from `len`, so a fixed-size
field is never silently truncated or padded.

### Custom tokens

Custom tokens can be added, suitable for reading binary files or network messages:
```js
  ExtendedHeader = {
    len: 10,

    get: (buf, off) => {
      return {
        // Extended header size
        size: Token.UINT32_BE.get(buf, off),
        // Extended Flags
        extendedFlags: Token.UINT16_BE.get(buf, off + 4),
        // Size of padding
        sizeOfPadding: Token.UINT32_BE.get(buf, off + 6),
        // CRC data present
        crcDataPresent: common.strtokBITSET.get(buf, off + 4, 31)
      };
    }
  };
```

## Licence

This project is licensed under the [MIT License](LICENSE.txt). Feel free to use, modify, and distribute as needed.
