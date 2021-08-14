use crate::rewards_math::*;
use fadroma::scrt::{cosmwasm_std::{StdError, CanonicalAddr}, storage::*};

macro_rules! error { ($info:expr) => { Err(StdError::generic_err($info)) }; } // just a shorthand

// storage keys for pool fields --------------------------------------------------------------------

/// How much liquidity has this pool contained up to this point.
/// On lock/unlock, if locked > 0 before the operation, this is incremented
/// in intervals of (moments since last update * current balance)
const POOL_LIFETIME:  &[u8] = b"/pool/lifetime";

/// How much liquidity is there in the whole pool right now.
/// Incremented/decremented on lock/unlock.
const POOL_LOCKED:    &[u8] = b"/pool/balance";

/// When was liquidity last updated.
/// Set to current time on lock/unlock.
const POOL_TIMESTAMP: &[u8] = b"/pool/updated";

/// Rewards claimed by everyone so far.
/// Incremented on claim.
const POOL_CLAIMED:   &[u8] = b"/pool/claimed";

#[cfg(feature="global_ratio")]
/// Ratio of liquidity provided to rewards received.
/// Configured on init.
const POOL_RATIO:     &[u8] = b"/pool/ratio";

#[cfg(feature="age_threshold")]
/// How much the user needs to wait before they can claim for the first time.
/// Configured on init.
const POOL_THRESHOLD: &[u8] = b"/pool/threshold";

#[cfg(feature="claim_cooldown")]
/// How much the user must wait between claims.
/// Configured on init.
const POOL_COOLDOWN:  &[u8] = b"/pool/cooldown";

#[cfg(feature="pool_liquidity_ratio")]
/// Store the moment the user is created to compute total pool existence.
/// Set on init.
const POOL_CREATED:   &[u8] = b"/pool/created";

#[cfg(feature="pool_liquidity_ratio")]
/// The first time a user locks liquidity,
/// this is set to the current time.
/// Used to calculate pool's liquidity ratio.
const POOL_POPULATED:   &[u8] = b"/pool/created";

#[cfg(feature="pool_liquidity_ratio")]
/// Used to compute what portion of the time the pool was not empty.
/// On lock/unlock, if the pool was not empty, this is incremented
/// by the time elapsed since the last update.
const POOL_LIQUID:    &[u8] = b"/pool/not_empty";

#[cfg(feature="pool_closes")]
/// Whether this pool is closed
const POOL_CLOSED:    &[u8] = b"/pool_closed";

// storage keys for user fields --------------------------------------------------------------------

/// How much liquidity has this user provided since they first appeared.
/// On lock/unlock, if the pool was not empty, this is incremented
/// in intervals of (moments since last update * current balance)
const USER_LIFETIME:  &[u8] = b"/user/lifetime/";

/// How much liquidity does this user currently provide.
/// Incremented/decremented on lock/unlock.
const USER_LOCKED:    &[u8] = b"/user/current/";

/// When did this user's liquidity amount last change
/// Set to current time on lock/unlock.
const USER_TIMESTAMP: &[u8] = b"/user/updated/";

/// How much rewards has each user claimed so far.
/// Incremented on claim.
const USER_CLAIMED:   &[u8] = b"/user/claimed/";

#[cfg(any(feature="age_threshold", feature="user_liquidity_ratio"))]
/// For how many units of time has this user provided liquidity
/// On lock/unlock, if locked was > 0 before the operation,
/// this is incremented by time elapsed since last update.
const USER_PRESENT:   &[u8] = b"/user/present/";

#[cfg(feature="user_liquidity_ratio")]
/// For how many units of time has this user been known to the contract.
/// Incremented on lock/unlock by time elapsed since last update.
const USER_EXISTED:   &[u8] = b"/user/existed/";

#[cfg(feature="claim_cooldown")]
/// For how many units of time has this user provided liquidity
/// Decremented on lock/unlock, reset to configured cooldown on claim.
const USER_COOLDOWN:  &[u8] = b"/user/cooldown/";

// structs implementing the rewards algorithm -----------------------------------------------------

/// Reward pool
pub struct Pool <S> {
    pub storage: S,
    now:         Option<Time>,
    balance:     Option<Amount>
}

/// User account
pub struct User <S> {
    pool:    Pool<S>,
    address: CanonicalAddr
}

impl <S> Pool<S> {
    /// Create a new pool with a storage handle
    pub fn new (storage: S) -> Self {
        Self { storage, now: None, balance: None }
    }
    /// Set the current time
    pub fn at (self, now: Time) -> Self {
        Self { now: Some(now), ..self }
    }
    /// Set the current balance
    pub fn with_balance (self, balance: Amount) -> Self {
        Self { balance: Some(balance), ..self }
    }
    /// Get an individual user from the pool
    pub fn user (self, address: CanonicalAddr) -> User<S> {
        User { pool: self, address }
    }
}

stateful!(Pool (storage):

    Readonly {

        // time-related getters --------------------------------------------------------------------

        /// Get the time since last update (0 if no last update)
        pub fn elapsed (&self) -> StdResult<Time> {
            // stop time when closing pool
            #[cfg(feature="pool_closes")]
            if self.closed()?.is_some() {
                return Ok(0 as Time)
            }

            Ok(self.now()? - self.timestamp()?) }

        /// Get the current time or fail
        pub fn now (&self) -> StdResult<Time> {
            self.now.ok_or(StdError::generic_err("current time not set")) }

        /// Load the last update timestamp or default to current time
        /// (this has the useful property of keeping `elapsed` zero for strangers)
        pub fn timestamp (&self) -> StdResult<Time> {
            match self.load(POOL_TIMESTAMP)? {
                Some(time) => Ok(time),
                None       => Ok(self.now()?) } }

        // lp token-related getters ----------------------------------------------------------------

        /// The total liquidity ever contained in this pool.
        pub fn lifetime (&self) -> StdResult<Volume> {
            tally(self.last_lifetime()?, self.elapsed()?, self.locked()?) }

        /// Snapshot of total liquidity at moment of last update.
        fn last_lifetime (&self) -> StdResult<Volume> {
            Ok(self.load(POOL_LIFETIME)?.unwrap_or(Volume::zero())) }

        /// Amount of currently locked LP tokens in this pool
        pub fn locked (&self) -> StdResult<Amount> {
            Ok(self.load(POOL_LOCKED)?.unwrap_or(Amount::zero())) }

        // reward-related getters ------------------------------------------------------------------

        /// Amount of rewards already claimed
        pub fn claimed (&self) -> StdResult<Amount> {
            Ok(self.load(POOL_CLAIMED)?.unwrap_or(Amount::zero())) }

        /// The full reward budget = rewards claimed + current balance of this contract in reward token
        pub fn budget (&self) -> StdResult<Amount> {
            Ok(self.claimed()? + self.balance()) }

        /// Current balance in reward token, or zero.
        pub fn balance (&self) -> Amount {
            self.balance.unwrap_or(Amount::zero()) }

        // balancing features ----------------------------------------------------------------------

        #[cfg(feature="age_threshold")]
        /// For how many blocks does the user need to have provided liquidity
        /// in order to be eligible for rewards
        pub fn threshold (&self) -> StdResult<Time> {
            match self.load(POOL_THRESHOLD)? {
                Some(threshold) => Ok(threshold),
                None            => error!("missing lock threshold") } }

        #[cfg(feature="claim_cooldown")]
        /// For how many blocks does the user need to wait
        /// after claiming rewards before being able to claim them again
        pub fn cooldown (&self) -> StdResult<Time> {
            match self.load(POOL_COOLDOWN)? {
                Some(cooldown) => Ok(cooldown),
                None           => error!("missing claim cooldown") } }

        #[cfg(feature="global_ratio")]
        /// Ratio between share of liquidity provided and amount of reward
        /// Should be <= 1 to make sure rewards budget is sufficient.
        pub fn ratio (&self) -> StdResult<Ratio> {
            match self.load(POOL_RATIO)? {
                Some(ratio) => Ok(ratio),
                None        => error!("missing reward ratio") } }

        #[cfg(feature="pool_liquidity_ratio")]
        /// Time for which the pool was not empty.
        pub fn liquid (&self) -> StdResult<Time> {
            Ok(if self.locked()? > Amount::zero() {
                self.last_liquid()? + self.elapsed()?
            } else {
                self.last_liquid()?
            }) }

        #[cfg(feature="pool_liquidity_ratio")]
        fn last_liquid (&self) -> StdResult<Time> {
            match self.load(POOL_LIQUID)? {
                Some(liquid) => Ok(liquid),
                None => Ok(0 as Time) } }

        #[cfg(feature="pool_liquidity_ratio")]
        pub fn liquidity_ratio (&self) -> StdResult<Amount> {
            let existed = self.now()? - self.populated()?;
            if existed == 0 {
                return Ok(Amount::from(HUNDRED_PERCENT)) }
            Ok(Volume::from(HUNDRED_PERCENT)
                .multiply_ratio(self.liquid()?, existed)?
                .low_u128().into()) }

        #[cfg(feature="pool_liquidity_ratio")]
        pub fn existed (&self) -> StdResult<Time> {
            Ok(self.now()? - self.populated()?) }

        #[cfg(feature="pool_liquidity_ratio")]
        fn populated (&self) -> StdResult<Time> {
            match self.load(POOL_POPULATED)? {
                Some(populated) => Ok(populated),
                None => Err(StdError::generic_err("nobody has locked any tokens yet")) } }

        #[cfg(feature="pool_liquidity_ratio")]
        fn created (&self) -> StdResult<Time> {
            match self.load(POOL_CREATED)? {
                Some(created) => Ok(created),
                None => Err(StdError::generic_err("missing creation date")) } }

        #[cfg(feature="pool_closes")]
        pub fn closed (&self) -> StdResult<Option<String>> {
            self.load(POOL_CLOSED) }

    }

    Writable {

        /// Increment the total amount of claimed rewards for all users.
        pub fn increment_claimed (&mut self, reward: Amount) -> StdResult<&mut Self> {
            self.save(POOL_CLAIMED, self.claimed()? + reward) }

        /// Every time the amount of tokens locked in the pool is updated,
        /// the pool's lifetime liquidity is tallied and and the timestamp is updated.
        /// This is the only user-triggered input to the pool.
        pub fn update_locked (&mut self, balance: Amount) -> StdResult<&mut Self> {
            // if this is the first time someone is locking tokens
            // store the timestamp. this is used to start
            // the liquidity ratio calculation from the time
            // of first lock instead of contract init
            match self.load(POOL_POPULATED)? as Option<Time> {
                None    => { self.save(POOL_POPULATED, self.now)?; },
                Some(0) => { self.save(POOL_POPULATED, self.now)?; },
                _ => {} };

            let lifetime = self.lifetime()?;
            let now      = self.now()?;

            #[cfg(feature="pool_liquidity_ratio")] {
                let liquid = self.liquid()?;
                self.save(POOL_LIQUID, liquid)?; }

            self.save(POOL_LIFETIME,  lifetime)?
                .save(POOL_TIMESTAMP, now)?
                .save(POOL_LOCKED,    balance) }

        // balancing features config ---------------------------------------------------------------

        #[cfg(feature="age_threshold")]
        pub fn configure_threshold (&mut self, threshold: &Time) -> StdResult<&mut Self> {
            self.save(POOL_THRESHOLD, threshold) }

        #[cfg(feature="claim_cooldown")]
        pub fn configure_cooldown (&mut self, cooldown: &Time) -> StdResult<&mut Self> {
            self.save(POOL_COOLDOWN, cooldown) }

        #[cfg(feature="global_ratio")]
        pub fn configure_ratio (&mut self, ratio: &Ratio) -> StdResult<&mut Self> {
            self.save(POOL_RATIO, ratio) }

        #[cfg(feature="pool_liquidity_ratio")]
        pub fn configure_populated (&mut self, time: &Time) -> StdResult<&mut Self> {
            self.save(POOL_POPULATED, time) }

        #[cfg(feature="pool_liquidity_ratio")]
        pub fn configure_created (&mut self, time: &Time) -> StdResult<&mut Self> {
            self.save(POOL_CREATED, time) }

        #[cfg(feature="pool_closes")]
        pub fn close (&mut self, message: String) -> StdResult<&mut Self> {
            self.save(POOL_CLOSED, Some(message)) }

    } );

stateful!(User (pool.storage):

    Readonly {

        // time-related getters --------------------------------------------------------------------

        /// Time of last lock or unlock
        pub fn timestamp (&self) -> StdResult<Option<Time>> {
            Ok(self.load_ns(USER_TIMESTAMP, self.address.as_slice())?) }

        #[cfg(any(feature="claim_cooldown", feature="user_liquidity_ratio"))]
        /// Time that progresses always. Used to increment existence.
        pub fn elapsed (&self) -> StdResult<Time> {
            // stop time when closing pool
            #[cfg(feature="pool_closes")]
            if self.pool.closed()?.is_some() {
                return Ok(0 as Time)
            }

            let now = self.pool.now()?;
            if let Ok(Some(timestamp)) = self.timestamp() {
                if now < timestamp { // prevent replay
                    return error!("no data")
                } else {
                    Ok(now - timestamp)
                }
            } else {
                Ok(0 as Time)
            }
        }

        /// Time that progresses only when the user has some tokens locked.
        /// Used to increment presence and lifetime.
        pub fn elapsed_present (&self) -> StdResult<Time> {
            if self.locked()? > Amount::zero() {
                self.elapsed()
            } else {
                Ok(0 as Time)
            }
        }

        #[cfg(feature="user_liquidity_ratio")]
        /// Up-to-date time for which the user has existed
        pub fn existed (&self) -> StdResult<Time> {
            Ok(self.last_existed()? + self.elapsed()?) }

        #[cfg(feature="user_liquidity_ratio")]
        /// Load last value of user existence
        fn last_existed (&self) -> StdResult<Time> {
            Ok(self.load_ns(USER_EXISTED, self.address.as_slice())?
                .unwrap_or(0 as Time)) }

        #[cfg(any(feature="age_threshold", feature="user_liquidity_ratio"))]
        /// Up-to-date time for which the user has provided liquidity
        pub fn present (&self) -> StdResult<Time> {
            Ok(self.last_present()? + self.elapsed_present()?) }

        #[cfg(any(feature="age_threshold", feature="user_liquidity_ratio"))]
        /// Load last value of user present
        fn last_present (&self) -> StdResult<Time> {
            Ok(self.load_ns(USER_PRESENT, self.address.as_slice())?
                .unwrap_or(0 as Time)) }

        #[cfg(feature="claim_cooldown")]
        pub fn cooldown (&self) -> StdResult<Time> {
            Ok(Time::saturating_sub(self.last_cooldown()?, self.elapsed()?)) }

        #[cfg(feature="claim_cooldown")]
        fn last_cooldown (&self) -> StdResult<Time> {
            Ok(self.load_ns(USER_COOLDOWN, self.address.as_slice())?
                .unwrap_or(self.pool.cooldown()?)) }

        // lp-related getters ----------------------------------------------------------------------

        pub fn locked (&self) -> StdResult<Amount> {
            Ok(self.load_ns(USER_LOCKED, self.address.as_slice())?.unwrap_or(Amount::zero())) }

        pub fn lifetime (&self) -> StdResult<Volume> {
            let mut locked = self.locked()?;

            #[cfg(feature="user_liquidity_ratio")] {
                let existed = self.existed()?;
                if existed == 0u64 {
                    return Ok(Volume::zero()) }
                locked = locked.multiply_ratio(self.present()?, existed); }

            tally(self.last_lifetime()?, self.elapsed_present()?, locked) }

        fn last_lifetime (&self) -> StdResult<Volume> {
            Ok(self.load_ns(USER_LIFETIME, self.address.as_slice())?.unwrap_or(Volume::zero())) }

        // reward-related getters ------------------------------------------------------------------

        // After first locking LP tokens, users must reach a configurable age threshold,
        // i.e. keep LP tokens locked for at least X blocks. During that time, their portion of
        // the total liquidity ever provided increases.
        //
        // The total reward for an user with an age under the threshold is zero.
        //
        // The total reward for a user with an age above the threshold is
        // (claimed_rewards + budget) * user_lifetime_liquidity / pool_lifetime_liquidity
        //
        // Since a user's total reward can diminish, it may happen that the amount claimed
        // by a user so far is larger than the current total reward for that user.
        // In that case the user's claimable amount remains zero until they unlock more
        // rewards than they've already claimed.
        //
        // Since a user's total reward can diminish, it may happen that the amount remaining
        // in the pool after a user has claimed is insufficient to pay out the next user's reward.
        // In that case, https://google.github.io/filament/webgl/suzanne.html

        pub fn share (&self, basis: u128) -> StdResult<Volume> {
            let pool_lifetime = self.pool.lifetime()?;
            if pool_lifetime == Volume::zero() {
                return Ok(Volume::zero()) }

            let mut share = Volume::from(basis);

            share = share.multiply_ratio(self.lifetime()?, pool_lifetime)?;

            #[cfg(feature="user_liquidity_ratio")] {
                let existed = self.existed()?;
                if existed == 0u64 { return Ok(Volume::zero()) }
                share = share.multiply_ratio(self.present()?, self.existed()?)?; }

            Ok(share) }

        pub fn earned (&self) -> StdResult<Amount> {
            let mut budget = self.pool.budget()?;

            #[cfg(feature="pool_liquidity_ratio")] {
                let existed = self.pool.existed()?;
                if existed == 0u64 { return Ok(Amount::zero()) }
                budget = budget.multiply_ratio(self.pool.liquid()?, existed); }

            #[cfg(feature="global_ratio")] {
                let ratio = self.pool.ratio()?;
                if ratio.1 == Amount::zero() { return Ok(Amount::zero()) }
                budget = budget.multiply_ratio(ratio.0, ratio.1); }

            Ok(self.share(budget.u128())?.low_u128().into()) }

        pub fn claimed (&self) -> StdResult<Amount> {
            Ok(self.load_ns(USER_CLAIMED, self.address.as_slice())?.unwrap_or(Amount::zero())) }

        pub fn claimable (&self) -> StdResult<Amount> {
            #[cfg(feature="age_threshold")]
            // you must lock for this long to claim
            if self.present()? < self.pool.threshold()? {
                return Ok(Amount::zero()) }

            let earned = self.earned()?;
            if earned == Amount::zero() {
                return Ok(Amount::zero()) }

            let claimed = self.claimed()?;
            if earned <= claimed {
                return Ok(Amount::zero()) }

            if let Some(balance) = self.pool.balance {
                let claimable = (earned - claimed)?;
                if claimable > balance {
                    return Ok(balance)
                } else {
                    return Ok(claimable)
                }
            } else {
                return Ok(Amount::zero())
            }
        }

    }

    Writable {

        // time-related mutations ------------------------------------------------------------------

        #[cfg(feature="claim_cooldown")]
        fn reset_cooldown (&mut self) -> StdResult<&mut Self> {
            let address = self.address.clone();
            self.save_ns(USER_COOLDOWN, address.as_slice(), self.pool.cooldown()?) }

        // lp-related mutations -------------------------------------------------------------------

        fn update (&mut self, user_balance: Amount, pool_balance: Amount) -> StdResult<()> {
            // Prevent replay
            let now = self.pool.now()?;
            if let Some(timestamp) = self.timestamp()? {
                if timestamp > now {
                    return error!("no data") } }

            // Commit rolling values to storage:

            let address = self.address.clone();

            #[cfg(feature="user_liquidity_ratio")] {
                // Increment existence
                let existed = self.existed()?;
                self.save_ns(USER_EXISTED, address.as_slice(), existed)?; }

            #[cfg(any(feature="age_threshold", feature="user_liquidity_ratio"))] {
                // Increment presence if user has currently locked tokens
                let present = self.present()?;
                self.save_ns(USER_PRESENT, address.as_slice(), present)?; }

            #[cfg(feature="claim_cooldown")] {
                // Cooldown is calculated since the timestamp.
                // Since we'll be updating the timestamp, commit the current cooldown
                let cooldown = self.cooldown()?;
                self.save_ns(USER_COOLDOWN, address.as_slice(), cooldown)?; }

            let lifetime = self.lifetime()?;
            self// Always increment lifetime
                .save_ns(USER_LIFETIME,  address.as_slice(), lifetime)?
                // Set user's time of last update to now
                .save_ns(USER_TIMESTAMP, address.as_slice(), now)?
                // Update amount locked
                .save_ns(USER_LOCKED,    address.as_slice(), user_balance)?
                // Update total amount locked in pool
                .pool.update_locked(pool_balance)?;

            Ok(()) }

        pub fn lock_tokens (&mut self, increment: Amount) -> StdResult<Amount> {
            let locked = self.locked()?;
            self.update(
                locked + increment,
                self.pool.locked()? + increment.into())?;
            // Return the amount to be transferred from the user to the contract
            Ok(increment) }

        pub fn retrieve_tokens (&mut self, decrement: Amount) -> StdResult<Amount> {
            // Must have enough locked to retrieve
            let locked = self.locked()?;
            if locked < decrement {
                return error!(format!("not enough locked ({} < {})", locked, decrement)) }
            self.update(
                (self.locked()? - decrement)?,
                (self.pool.locked()? - decrement.into())?)?;
            // Return the amount to be transferred back to the user
            Ok(decrement) }

        // reward-related mutations ----------------------------------------------------------------

        fn increment_claimed (&mut self, reward: Amount) -> StdResult<&mut Self> {
            let address = self.address.clone();
            self.save_ns(USER_CLAIMED, address.as_slice(), self.claimed()? + reward) }

        pub fn claim_reward (&mut self) -> StdResult<Amount> {
            #[cfg(feature="age_threshold")]
            // If user must wait before their first claim, enforce that here.
            enforce_cooldown(self.present()?, self.pool.threshold()?)?;

            #[cfg(feature="claim_cooldown")]
            // If user must wait between claims, enforce that here.
            enforce_cooldown(0, self.cooldown()?)?;

            // See if there is some unclaimed reward amount:
            let claimable = self.claimable()?;
            if claimable == Amount::zero() {
                return error!(
                    "You've already received as much as your share of the reward pool allows. \
                    Keep your liquidity tokens locked and wait for more rewards to be vested, \
                    and/or lock more liquidity tokens to grow your share of the reward pool.") }

            // Now we need the user's liquidity token balance for two things:
            let locked = self.locked()?;

            // 1. Update the user timestamp, and the other things that may be synced to it.
            //    Sacrifices efficiency (gas cost for a few more load/save operations than
            //    the absolute minimum) for an avoidance of hidden dependencies.
            self.update(locked, self.pool.locked()?)?;

            // (In the meantime, update how much has been claimed...
            self.increment_claimed(claimable)?;
            self.pool.increment_claimed(claimable)?;

            // ...and, optionally, reset the cooldown so that
            // the user has to wait before claiming again)
            #[cfg(feature="claim_cooldown")]
            self.reset_cooldown()?; // Reset the user cooldown

            // 2. Optionally, reset the user's `lifetime` and `share` if they have currently
            //    0 tokens locked. The intent is for this to be the user's last reward claim
            //    after they've left the pool completely. If they provide exactly 0 liquidity
            //    at some point, when they come back they have to start over, which is OK
            //    because they can then start claiming rewards immediately, without waiting
            //    for threshold, only cooldown.
            #[cfg(feature="selective_memory")] {
                if locked == Amount::zero() {
                    let address = self.address.clone();
                    self.save_ns(USER_LIFETIME, address.as_slice(), Volume::zero())?;
                    self.save_ns(USER_CLAIMED,  address.as_slice(), Volume::zero())?;
                }
            }

            // Return the amount that the contract will send to the user
            Ok(claimable)
        }
    }
);

#[cfg(any(feature="claim_cooldown", feature="age_threshold"))]
fn enforce_cooldown (elapsed: Time, cooldown: Time) -> StdResult<()> {
    if elapsed >= cooldown {
        Ok(())
    } else {
        error!(format!("lock tokens for {} more blocks to be eligible", cooldown - elapsed))
    }
}
