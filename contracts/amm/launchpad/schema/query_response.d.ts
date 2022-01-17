/* tslint:disable */
/**
 * This file was automatically generated by json-schema-to-typescript.
 * DO NOT MODIFY IT BY HAND. Instead, modify the source JSONSchema file,
 * and run json-schema-to-typescript to regenerate this file.
 */

export type QueryResponse =
  | {
      launchpad_info: QueryTokenConfig[];
      [k: string]: unknown;
    }
  | {
      user_info: QueryAccountToken[];
      [k: string]: unknown;
    }
  | {
      drawn_addresses: HumanAddr[];
      [k: string]: unknown;
    };
export type Uint128 = string;
export type TokenType =
  | {
      custom_token: {
        contract_addr: HumanAddr;
        token_code_hash: string;
        [k: string]: unknown;
      };
      [k: string]: unknown;
    }
  | {
      native_token: {
        denom: string;
        [k: string]: unknown;
      };
      [k: string]: unknown;
    };
export type HumanAddr = string;

/**
 * Token configuration that holds the configuration for each token
 */
export interface QueryTokenConfig {
  bounding_period: number;
  locked_balance: Uint128;
  segment: Uint128;
  token_decimals: number;
  token_type: TokenType;
  [k: string]: unknown;
}
/**
 * Account token representation that holds all the entries for this token
 */
export interface QueryAccountToken {
  balance: Uint128;
  entries: number[];
  token_type: TokenType;
  [k: string]: unknown;
}
