import assert from 'node:assert/strict'
import { test } from 'node:test'
import { offeredHosts, parseHosts } from './ssh-config.ts'

test('ssh config hosts: named hosts with their first HostName and User, includes kept', () => {
  const { hosts, includes } = parseHosts(`
Include config.d/*
# a comment
Host mac-mini
  HostName 100.126.77.71
  User leepokai
  HostName ignored
Host isc-cluster romeo1
    HostName=romeo1.univ-reims.fr
Host *.internal !bad ??
  User nobody
Host tenfoldai
  HostName github.com
  User git
Match host foo
  User someone
Host -oProxyCommand=evil
Host *
  ServerAliveInterval 30
`)
  assert.deepEqual(includes, ['config.d/*'])
  assert.deepEqual(hosts, [
    { alias: 'mac-mini', hostname: '100.126.77.71', user: 'leepokai' },
    { alias: 'isc-cluster', hostname: 'romeo1.univ-reims.fr', user: null },
    { alias: 'romeo1', hostname: 'romeo1.univ-reims.fr', user: null },
    { alias: 'tenfoldai', hostname: 'github.com', user: 'git' },
  ])
  assert.deepEqual(offeredHosts([...hosts, { alias: 'mac-mini', hostname: 'other', user: null }]).map((h) => h.alias), ['mac-mini', 'isc-cluster', 'romeo1'])
})
