import { Console, bold, Scrt_1_2, Snip20Contract, randomHex } from "@hackbg/fadroma"

const console = Console('@sienna/rewards/Contract')

import { RewardsAPIVersion, RewardsClient } from './RewardsClient'
export * from './RewardsClient'

const {
  SIENNA_REWARDS_V2_BONDING,
  SIENNA_REWARDS_V3_BONDING
} = process.env

import { Init } from './schema/init.d'
import { workspace } from '@sienna/settings'
import { LPTokenContract } from '@sienna/lp-token'
import { RPTContract } from '@sienna/rpt'
export abstract class RewardsContract extends Scrt_1_2.Contract<RewardsClient> {

  name   = 'Rewards'
  source = { workspace, crate: 'sienna-rewards' }
  abstract Client
  abstract version:        RewardsAPIVersion
  abstract rewardToken (): Promise<Snip20Contract>
  abstract lpToken     (): Promise<Snip20Contract>

  static "v2" = class RewardsContract_v2 extends RewardsContract {

    name    = `Rewards[${this.version}]`
    source = { workspace, crate: 'sienna-rewards', ref: 'rewards-2.1.2' }
    version = "v2" as RewardsAPIVersion
    initMsg?: any // TODO v2 init type
    Client = RewardsClient['v2']

    constructor (input) {
      super()
      const { lpToken, rewardToken, agent } = input
      if (SIENNA_REWARDS_V2_BONDING) {
        console.warn(bold('Environment override'), 'SIENNA_REWARDS_V2_BONDING=', SIENNA_REWARDS_V2_BONDING)
      }
      this.initMsg = {
        admin:        agent?.address,
        lp_token:     lpToken?.link,
        reward_token: rewardToken?.link,
        viewing_key:  "",
        ratio:        ["1", "1"],
        threshold:    Number(SIENNA_REWARDS_V2_BONDING||15940),
        cooldown:     Number(SIENNA_REWARDS_V2_BONDING||15940),
      }
    }

    async lpToken <T extends LPTokenContract> (T = LPTokenContract): Promise<T> {
      const at = Math.floor(+new Date()/1000)
      const {pool_info} = await this.query({pool_info:{at}})
      const {address, code_hash} = pool_info.lp_token
      return new T({ address, codeHash: code_hash, agent: this.agent }) as T
    }

    async rewardToken <T extends Snip20Contract> (T = Snip20Contract): Promise<T> {
      throw new Error('not implemented')
    }

    /** Command. Deploy legacy Rewards v2. */
    static deploy = async function deployRewards_v2 ({run}) {
      const { RPT_CONFIG, REWARD_POOLS } =
        await run(RewardsContract.deployRewards, { version: 'v2' })
      await run(RPTContract.adjustConfig, { RPT_CONFIG })
      return { RPT_CONFIG, REWARD_POOLS }
    }

    /** Command. Replace v2 reward pools with v3. */
    static upgrade = {
      "v3": function upgradeRewards_v2_to_v3 (input) {
        return RewardsContract.upgradeRewards({
          ...input,
          oldVersion: 'v2', OldRewardsContract: RewardsContract.v2,
          newVersion: 'v3', NewRewardsContract: RewardsContract.v3,
        })
      }
    }
  }

  static "v3" = class RewardsContract_v3 extends RewardsContract {

    version = "v3" as RewardsAPIVersion
    name    = `Rewards[${this.version}]`
    initMsg?: Init
    Client = RewardsClient['v3']
    constructor (input) {
      super(input)
      const { lpToken, rewardToken, agent } = input
      if (SIENNA_REWARDS_V3_BONDING) {
        console.warn(bold('Environment override'), 'SIENNA_REWARDS_V3_BONDING=', SIENNA_REWARDS_V3_BONDING)
      }
      this.initMsg = {
        admin: agent?.address,
        config: {
          reward_vk:    randomHex(36),
          bonding:      Number(process.env.SIENNA_REWARDS_V3_BONDING||86400),
          timekeeper:   agent?.address,
          lp_token:     lpToken?.link,
          reward_token: rewardToken?.link,
        }
      }
    }

    async lpToken <T extends Snip20Contract> (T = LPTokenContract): Promise<T> {
      const { lp_token: { address, code_hash } } = await this.q().config()
      return new T({ address, codeHash: code_hash, agent: this.agent }) as T
    }

    async rewardToken <T extends Snip20Contract> (T = Snip20Contract): Promise<T> {
      throw new Error('not implemented')
    }

    get epoch (): Promise<number> {
      return this.q().pool_info().then(pool_info=>pool_info.clock.number)
    }

    /** Command. Deploy Rewards v3. */
    static deploy = async function deployRewards_v3 ({run}) {
      const { RPT_CONFIG, REWARD_POOLS } = await run(RewardsContract.deployRewards, { version: 'v3' })
      return await run(RPTContract.adjustConfig, { RPT_CONFIG })
    }

    /** Command. v3 to v3 upgrade tests user migration. */
    static upgrade = {
      "v3": function upgradeRewards_v2_to_v3 (input) {
        return RewardsContract.upgradeRewards({
          ...input,
          oldVersion: 'v3', OldRewardsContract: RewardsContract.v3,
          newVersion: 'v3', NewRewardsContract: RewardsContract.v3,
        })
      }
    }
  }

  static "v2+v3" = {
    /** Command. Deploy both versions simultaneously,
      * splitting the balance evenly in the RPT config. */
    deploy: deployRewards_v2_and_v3
  }

  /** Deploy a single reward pool. */
  static deployOne = deployRewardPool

  /** Command. Attach a specified version of Sienna Rewards
    * to a specified version of Sienna Swap. */
  static deployRewards = deployRewards

  static upgradeRewards = upgradeRewards

}

import { SiennaSnip20Contract } from '@sienna/snip20-sienna'
import { MigrationContext, Snip20Client } from '@hackbg/fadroma'
import { LPTokenClient } from '@sienna/lp-token'
async function deployRewardPool ({
  agent, deployment, prefix,
  lpToken,
  rewardToken = new SiennaSnip20Contract(deployment.get('SIENNA')).client(agent),
  name        = 'UNKNOWN',
  version     = 'v3',
}: MigrationContext & {
  lpToken:     LPTokenClient,
  rewardToken: Snip20Client,
  name:        string,
  version:     RewardsAPIVersion
}): Promise<RewardsClient> {
  const Contract = RewardsContract[version]
  const contract = new Contract({ lpToken, rewardToken })
  await agent.chain.buildAndUpload(agent, [contract])
  contract.name = `${name}.Rewards[${version}]`
  await deployment.getOrInit(agent, contract)
  return contract.client(agent)
}

import { RPTConfig } from '@sienna/rpt'
import { AMMVersion } from '@sienna/exchange'
async function deployRewards ({
  deployment, agent, run,
  SIENNA     = new SiennaSnip20Contract(deployment.get('SIENNA')).client(agent),
  version    = 'v3' as RewardsAPIVersion,
  ammVersion = { v3: 'v2', v2: 'v1' }[version] as AMMVersion,
}: MigrationContext & {
  SIENNA:     Snip20Client,
  version:    RewardsAPIVersion,
  ammVersion: AMMVersion
}): Promise<{
  REWARD_POOLS: RewardsClient[],
  RPT_CONFIG:   RPTConfig
}> {
  const result = { REWARD_POOLS: [], RPT_CONFIG: [] }
  await agent.bundle().wrap(async bundle=>{
    console.log('SSSSS')
    const { SSSSS_POOL, RPT_CONFIG_SSSSS } =
      await run(deploySSSSS, { SIENNA, version, agent: bundle })
    console.log('REWARD_POOLS')
    const { REWARD_POOLS, RPT_CONFIG_SWAP_REWARDS } =
      await run(deployRewardPools, { SIENNA, version, ammVersion, agent: bundle })
    result.REWARD_POOLS = [ SSSSS_POOL, ...REWARD_POOLS ] 
    result.RPT_CONFIG   = [ ...RPT_CONFIG_SSSSS, ...RPT_CONFIG_SWAP_REWARDS ]
  })
  console.log(5, result)
  return result
}

import getSettings from '@sienna/settings'

/** Deploy SIENNA/SIENNA SINGLE-SIDED STAKING,
  * where you stake SIENNA to earn SIENNA. */
export async function deploySSSSS ({
  run, deployment, agent,
  SIENNA                    = new SiennaSnip20Contract(deployment.get('SIENNA')).client(agent),
  version                   = 'v3',
  settings: { rewardPairs } = getSettings(agent.chain.id),
}) {
  if (!rewardPairs || rewardPairs.length === 1) {
    throw new Error(`@sienna/rewards: needs rewardPairs setting for ${agent.chain.id}`)
  }
  const lpToken     = SIENNA
  const rewardToken = SIENNA
  const { REWARDS } = await run(RewardsContract.deployOne, {
    agent,
    version,
    name: 'SIENNA',
    lpToken: SIENNA
  })
  return {
    SSSSS_POOL: REWARDS, RPT_CONFIG_SSSSS: [
      [
        REWARDS.address,
        String(BigInt(getSettings(agent.chain.id).rewardPairs.SIENNA) * ONE_SIENNA)
      ]
    ]
  }
}

import { ONE_SIENNA } from '@sienna/settings'

/** Deploy the rest of the reward pools, where you stake a LP token to earn SIENNA. */
export async function deployRewardPools ({
  agent, deployment, prefix, run,
  SIENNA                    = new SiennaSnip20Contract(deployment.get('SIENNA')).client(agent),
  version                   = 'v3',
  ammVersion                = {v3:'v2',v2:'v1'}[version],
  settings: { rewardPairs } = getSettings(agent.chain.id),
  REWARD_POOLS              = [],
  split                     = 1.0,
  RPT_CONFIG_SWAP_REWARDS   = [],
}) {
  if (!rewardPairs || rewardPairs.length === 1) {
    throw new Error(`@sienna/rewards: needs rewardPairs setting for ${agent.chain.id}`)
  }
  for (let [name, reward] of Object.entries(rewardPairs)) {
    // ignore SSSSS pool - that is deployed separately
    if (name === 'SIENNA') continue
    // find LP token to attach to
    const lpTokenName = `AMM[${ammVersion}].${name}.LP`
    const lpToken = new LPTokenContract(deployment.get(lpTokenName))
    // create a reward pool
    const options = { version, name: lpTokenName, lpToken, agent }
    //console.info('Deploying', bold(name), version, 'for', bold(lpTokenName))
    const { REWARDS } = await run(RewardsContract.deployOne, options)
    REWARD_POOLS.push(REWARDS)
    // collect the RPT budget line
    const reward = BigInt(rewardPairs[name]) / BigInt(1 / split)
    const budget = [REWARDS.address, String(reward * ONE_SIENNA)]
    RPT_CONFIG_SWAP_REWARDS.push(budget)
  }
  return { REWARD_POOLS, RPT_CONFIG_SWAP_REWARDS }
}

async function upgradeRewards (context) {
  const {
    timestamp, agent, deployment, prefix, run,
    oldVersion, OldRewardsContract,
    newVersion, NewRewardsContract,
    SIENNA = new SiennaSnip20Contract(deployment.get('SIENNA')).client(agent),
    RPT    = new RPTContract(deployment.get('RPT')).client(agent),
    REWARD_POOLS = deployment.getAll('Rewards[v2].', name => new OldRewardsContract({agent})),
    version,
    suffix = `+${timestamp}`
  } = context
  const NEW_REWARD_POOLS: RewardsContract[] = []
  for (const REWARDS of REWARD_POOLS) {
    //console.log({REWARDS})
    //console.log(REWARDS.lpToken())
    //process.exit(123)
    const LP_TOKEN = REWARDS.lpToken
    const {symbol} = await LP_TOKEN.info
    let name
    if (symbol === 'SIENNA') {
      name = 'SIENNA'
    } else {
      const [LP, TOKEN0, TOKEN1] = (await LP_TOKEN.friendlyName).split('-')
      name = `AMM[v2].${TOKEN0}-${TOKEN1}.LP`
    }
    //console.log()
    //console.info(bold('Upgrading reward pool'), name)
    const { REWARDS: NEW_REWARDS } = await run(RewardsContract.deployOne, {
      version,
      name,
      lpToken: LP_TOKEN,
      rewardToken: SIENNA
    })
    NEW_REWARD_POOLS.push(NEW_REWARDS)
  }
  console.info(`Deployed`, bold(String(NEW_REWARD_POOLS.length)), version, `reward pools.`)
  return { REWARD_POOLS: NEW_REWARD_POOLS }
}

type MultisigTX = any
const pick       = (...keys) => x => keys.reduce((y, key)=>{y[key]=x[key];return y}, {})
const essentials = pick('codeId', 'codeHash', 'address', 'label')


async function deployRewards_v2_and_v3 ({run}) {
  const [V2, V3] = await Promise.all([
    run(RewardsContract.deployRewards, { version: 'v2' }),
    run(RewardsContract.deployRewards, { version: 'v3' })
  ])
  const REWARD_POOLS = [ ...V2.REWARD_POOLS, ...V3.REWARD_POOLS ]
  console.table(REWARD_POOLS.reduce(
    (table, {label, address, codeId, codeHash})=>
      Object.assign(table, {
        [label]: { address: address, codeId: codeId, codeHash: codeHash }
      }), {}))
  const RPT_CONFIG  = [ ...V2.RPT_CONFIG,   ...V3.RPT_CONFIG   ]
  return await run(RPTContract.adjustConfig, { RPT_CONFIG })
  return { RPT_CONFIG, REWARD_POOLS }
}
