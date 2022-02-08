use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::algo::{self, Account, IAccount};
use crate::auth::Auth;
use crate::keplr::KeplrCompat;
use fadroma::*;

use super::validator;
use super::{
    config::{GovernanceConfig, IGovernanceConfig},
    expiration::Expiration,
    governance::Governance,
    poll::{IPoll, Poll, PollStatus},
    poll_metadata::PollMetadata,
    vote::{IVote, Vote, VoteType},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceHandle {
    CreatePoll { meta: PollMetadata },
    Vote { variant: VoteType, poll_id: u64 },
    Unvote { poll_id: u64 },
    ChangeVote { variant: VoteType, poll_id: u64 },
    UpdateConfig { config: GovernanceConfig },
    Reveal { poll_id: u64 },
}
impl<S, A, Q, C> HandleDispatch<S, A, Q, C> for GovernanceHandle
where
    S: Storage,
    A: Api,
    Q: Querier,
    C: Governance<S, A, Q>,
{
    fn dispatch_handle(self, core: &mut C, env: Env) -> StdResult<HandleResponse> {
        match self {
            GovernanceHandle::CreatePoll { meta } => {
                validator::validate_text_length(
                    &meta.title,
                    "Title",
                    GovernanceConfig::MIN_TITLE_LENGTH,
                    GovernanceConfig::MAX_TITLE_LENGTH,
                )?;
                validator::validate_text_length(
                    &meta.description,
                    "Description",
                    GovernanceConfig::MIN_DESC_LENGTH,
                    GovernanceConfig::MAX_DESC_LENGTH,
                )?;
                let account = algo::Account::from_env(core, &env)?;
                let threshold = GovernanceConfig::threshold(core)?;
                if account.staked < threshold.into() {
                    return Err(StdError::generic_err("Insufficient funds to create a poll"));
                };

                let id = Poll::create_id(core)?;
                let deadline = GovernanceConfig::deadline(core)?;
                let expiration = Expiration::AtTime(env.block.time + deadline);

                let poll = Poll {
                    creator: core.canonize(env.message.sender)?,
                    expiration,
                    id,
                    metadata: meta,
                    status: PollStatus::Active,
                    reveal_approvals: vec![],
                };

                poll.store(core)?;

                Ok(HandleResponse {
                    data: Some(to_binary(&poll)?),
                    log: vec![
                        log("ACTION", "CREATE_POLL"),
                        log("POLL_ID", format!("{}", id)),
                        log("POLL_CREATOR", format!("{}", &poll.creator)),
                    ],
                    messages: vec![],
                })
            }
            GovernanceHandle::Vote { variant, poll_id } => {
                if let Ok(_) = Vote::get(core, env.message.sender.clone(), poll_id) {
                    return Err(StdError::generic_err(
                        "Already voted. Did you mean to update vote?",
                    ));
                }
                let account = Account::from_env(core, &env)?;

                let vote_power = account.staked;

                let vote = Vote {
                    variant,
                    vote_power,
                    voter: core.canonize(env.message.sender.clone())?,
                };

                vote.store(core, env.message.sender, poll_id)?;

                //calculate current tally/result

                Ok(HandleResponse::default())
            }
            GovernanceHandle::ChangeVote {
                variant: _,
                poll_id: _,
            } => Ok(HandleResponse::default()),
            GovernanceHandle::Unvote { poll_id: _ } => Ok(HandleResponse::default()),
            GovernanceHandle::Reveal { poll_id: _ } => {
                // not implemented
                Ok(HandleResponse::default())
            }
            _ => {
                Auth::assert_admin(core, &env)?;
                match self {
                    GovernanceHandle::UpdateConfig { config } => Ok(HandleResponse {
                        messages: config.store(core)?,
                        log: vec![],
                        data: None,
                    }),
                    _ => unreachable!(),
                }
            }
        }
    }
}
