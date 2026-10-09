// electron-builder merges inherited arrays, which would let Dev register production callbacks.
// Compose the two objects ourselves so variant-owned resource and protocol lists replace them.
const { readFileSync } = require('node:fs')
const { join } = require('node:path')
const { parse } = require('yaml')
const root = join(__dirname, '..')
const base = parse(readFileSync(join(root, 'electron-builder.yml'), 'utf8'))
const dev = parse(readFileSync(join(root, 'electron-builder.dev.yml'), 'utf8'))
module.exports = {
  ...base,
  ...dev,
  mac: { ...base.mac, ...dev.mac },
  dmg: { ...base.dmg, ...dev.dmg },
  linux: { ...base.linux, ...dev.linux },
  win: { ...base.win, ...dev.win },
}
