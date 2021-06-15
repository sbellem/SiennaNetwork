import open from 'open'
import { bold } from '@fadroma/utilities'
import SecretNetwork from '@fadroma/scrt-agent/network.js'

import { abs, projectRoot } from './root.js'
import { args, combine } from './args.js'
import { fmtSIENNA } from './decimals.js'
import { genCoverage, genSchema, genDocs } from './gen.js'
import { cargo, runTests, runDemo } from './run.js'

export function ensureWallets (context = {}) {
  console.warn('not implemented')
}

export async function selectLocalnet (context = {}) {
  console.debug(`Running on ${bold('localnet')}:`)
  context.network = await SecretNetwork.localnet()
}

export function resetLocalnet (context = {}) {
  return new Localnet().terminate()
}

export async function selectTestnet (context = {}) {
  console.debug(`Running on ${bold('testnet')}:`)
  context.network = await SecretNetwork.testnet()
}

export async function selectMainnet (context = {}) {
  console.debug(`Running on ${bold('mainnet')}:`)
  context.network = await SecretNetwork.localnet()
}

export function openFaucet () {
  const url = `https://faucet.secrettestnet.io/`
  console.debug(`Opening ${url}...`)
  open(url)
}

export {
  abs,
  args,
  cargo,
  combine,
  fmtSIENNA,
  projectRoot,
  genCoverage,
  genSchema,
  genDocs,
  runTests,
  runDemo
}
