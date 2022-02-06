import { Console, bold } from '@hackbg/fadroma'
const console = Console('@sienna/snip20-sienna')

import { Snip20Contract_1_0, Snip20Client, Source } from '@hackbg/fadroma'
import { workspace } from '@sienna/settings'
import { SiennaSnip20Client } from './SiennaSnip20Client'
export { SiennaSnip20Client }
export class SiennaSnip20Contract extends Snip20Contract_1_0 {
  name   = 'SIENNA'
  source = { workspace, crate: 'snip20-sienna' }
  Client = SiennaSnip20Client
  static status = siennaStatus
}

async function siennaStatus (context) {
  const {
    deployment, agent,
    sienna = deployment.get('SIENNA', SiennaSnip20Contract).client(agent)
  } = context
  try {
    const balance = await sienna.balance(agent.address, '')
    console.info(`SIENNA balance of ${bold(agent.address)}: ${balance}`)
  } catch (e) {
    console.error(e.message)
  }
}
