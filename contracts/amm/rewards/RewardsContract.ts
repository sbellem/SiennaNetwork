import {
  Scrt_1_2,
  Agent, Contract,
  ContractOptions, ContractConstructor,
  randomHex
} from "@hackbg/fadroma"
import { SNIP20Contract } from '@fadroma/snip20'
import { Init } from './schema/init.d'

import { RewardsTransactions } from './RewardsTransactions'
import { RewardsQueries } from './RewardsQueries'

export class RewardsContract extends Scrt_1_2.Contract<RewardsTransactions, RewardsQueries> {

  crate = 'sienna-rewards'

  name = 'SiennaRewards'

  initMsg: Init = {
    admin: this.creator?.address,
    config: {}
  }

  admin?: Agent

  constructor (options: ContractOptions & {
    /** Admin agent */
    admin?:       Agent,
    /** Address of other user that can increment the epoch */
    timekeeper?:  string,
    /** Staked token */
    lpToken?:     SNIP20Contract,
    /** Rewarded token */
    rewardToken?: SNIP20Contract,
    /** Bonding period config */
    bonding?:     number,
  } = {}) {
    super(options)
    const { name, admin } = options
    if (name) this.name = name // why
    if (admin) {
      this.admin = admin
      this.initMsg.admin = admin.address
    }
    this.initMsg.config = {
      reward_vk:    randomHex(36),
      bonding:      options.bonding || 86400,
      timekeeper:   options.timekeeper,
      lp_token:     options.lpToken?.link,
      reward_token: options.rewardToken?.link,
    }
  }

  get epoch (): Promise<number> {
    return this.q().pool_info().then(pool_info=>pool_info.clock.number)
  }

  RewardTokenContract: ContractConstructor<SNIP20Contract> = SNIP20Contract
  async rewardToken <T extends SNIP20Contract>(SNIP20 = this.RewardTokenContract) {
    const { address, code_hash } = (await this.q().pool_info()).reward_token
    return new SNIP20({ address, codeHash: code_hash, admin: this.admin })
  }

  LPTokenContract: ContractConstructor<SNIP20Contract> = SNIP20Contract
  async lpToken <T extends SNIP20Contract>(SNIP20 = this.LPTokenContract) {
    const { address, code_hash } = (await this.q().pool_info()).lp_token
    return new SNIP20({ address, codeHash: code_hash, admin: this.admin })
  }

}

export type RewardsAPIVersion = 'v2'|'v3'
