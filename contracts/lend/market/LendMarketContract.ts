import { Scrt_1_2 } from "@hackbg/fadroma"
import { workspace } from "@sienna/settings"
import { InitMsg } from './schema/init_msg.d'

export class LendMarketContract extends Scrt_1_2.Contract<any> {
  name   = 'SiennaLendMarket'
  source = { workspace, crate: 'lend-market' }
  initMsg?: InitMsg
}
