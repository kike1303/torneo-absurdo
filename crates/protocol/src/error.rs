use serde::{Deserialize, Serialize};

/// HTTP error codes (`http.*`) of the API (RF-B5, RF-B12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ApiErrorCode {
    #[serde(rename = "http.not_found")]
    NotFound,
    #[serde(rename = "http.method_not_allowed")]
    MethodNotAllowed,
    #[serde(rename = "http.rate_limited")]
    RateLimited,
    /// Any rejected JSON body: malformed, wrong `Content-Type`, unknown field, wrong type, a
    /// value outside the enum or over the size limit (008 RF-C5, amendment 2026-10-29).
    #[serde(rename = "http.invalid_body")]
    InvalidBody,
}

impl ApiErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[
        Self::NotFound,
        Self::MethodNotAllowed,
        Self::RateLimited,
        Self::InvalidBody,
    ];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "http.";

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "http.not_found",
            Self::MethodNotAllowed => "http.method_not_allowed",
            Self::RateLimited => "http.rate_limited",
            Self::InvalidBody => "http.invalid_body",
        }
    }
}

/// WebSocket codes (`ws.*`): HTTP bodies of the handshake (403, 503) and close reasons
/// (RF-B5, RF-B13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum WsErrorCode {
    #[serde(rename = "ws.origin_forbidden")]
    OriginForbidden,
    #[serde(rename = "ws.capacity")]
    Capacity,
    #[serde(rename = "ws.invalid_message")]
    InvalidMessage,
    #[serde(rename = "ws.echo_disabled")]
    EchoDisabled,
    #[serde(rename = "ws.rate_limited")]
    RateLimited,
    #[serde(rename = "ws.pong_timeout")]
    PongTimeout,
    #[serde(rename = "ws.idle_timeout")]
    IdleTimeout,
    #[serde(rename = "ws.ip_capacity")]
    IpCapacity,
    #[serde(rename = "ws.user_capacity")]
    UserCapacity,
    #[serde(rename = "ws.forbidden_channel")]
    ForbiddenChannel,
    #[serde(rename = "ws.too_many_subscriptions")]
    TooManySubscriptions,
}

impl WsErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[
        Self::OriginForbidden,
        Self::Capacity,
        Self::InvalidMessage,
        Self::EchoDisabled,
        Self::RateLimited,
        Self::PongTimeout,
        Self::IdleTimeout,
        Self::IpCapacity,
        Self::UserCapacity,
        Self::ForbiddenChannel,
        Self::TooManySubscriptions,
    ];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "ws.";

    /// The serialized code (the same string serde writes); also the WebSocket close reason.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OriginForbidden => "ws.origin_forbidden",
            Self::Capacity => "ws.capacity",
            Self::InvalidMessage => "ws.invalid_message",
            Self::EchoDisabled => "ws.echo_disabled",
            Self::RateLimited => "ws.rate_limited",
            Self::PongTimeout => "ws.pong_timeout",
            Self::IdleTimeout => "ws.idle_timeout",
            Self::IpCapacity => "ws.ip_capacity",
            Self::UserCapacity => "ws.user_capacity",
            Self::ForbiddenChannel => "ws.forbidden_channel",
            Self::TooManySubscriptions => "ws.too_many_subscriptions",
        }
    }
}

/// Authentication codes (`auth.*`, feature 008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum AuthErrorCode {
    #[serde(rename = "auth.csrf_origin")]
    CsrfOrigin,
    #[serde(rename = "auth.invalid_state")]
    InvalidState,
    #[serde(rename = "auth.banned")]
    Banned,
    #[serde(rename = "auth.provider_error")]
    ProviderError,
    #[serde(rename = "auth.try_again")]
    TryAgain,
    #[serde(rename = "auth.unauthenticated")]
    Unauthenticated,
    /// The identity of a link is already another user's (008 RF-C2): never merged.
    #[serde(rename = "auth.already_linked")]
    AlreadyLinked,
    /// The user already has an account of that provider (008 RF-C2).
    #[serde(rename = "auth.provider_already_linked")]
    ProviderAlreadyLinked,
    /// Unlinking would leave no enabled provider to log in with (008 RF-C3).
    #[serde(rename = "auth.last_provider")]
    LastProvider,
    /// The action needs a recent authentication that counts (ADR-0008 D9, amendment
    /// 2026-10-30; 009 RF8, RF9, RF13, RF16).
    #[serde(rename = "auth.reauth_required")]
    ReauthRequired,
    /// A reauthentication came back with an identity that is not one of the user's: absent or
    /// another user's, the same code (009 RF11, RF12).
    #[serde(rename = "auth.reauth_mismatch")]
    ReauthMismatch,
    /// `logout-all` without a valid session closed no other session (TA-37).
    #[serde(rename = "auth.sessions_not_closed")]
    SessionsNotClosed,
}

impl AuthErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[
        Self::CsrfOrigin,
        Self::InvalidState,
        Self::Banned,
        Self::ProviderError,
        Self::TryAgain,
        Self::Unauthenticated,
        Self::AlreadyLinked,
        Self::ProviderAlreadyLinked,
        Self::LastProvider,
        Self::ReauthRequired,
        Self::ReauthMismatch,
        Self::SessionsNotClosed,
    ];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "auth.";

    /// The serialized code (the same string serde writes); also a WebSocket close reason.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CsrfOrigin => "auth.csrf_origin",
            Self::InvalidState => "auth.invalid_state",
            Self::Banned => "auth.banned",
            Self::ProviderError => "auth.provider_error",
            Self::TryAgain => "auth.try_again",
            Self::Unauthenticated => "auth.unauthenticated",
            Self::AlreadyLinked => "auth.already_linked",
            Self::ProviderAlreadyLinked => "auth.provider_already_linked",
            Self::LastProvider => "auth.last_provider",
            Self::ReauthRequired => "auth.reauth_required",
            Self::ReauthMismatch => "auth.reauth_mismatch",
            Self::SessionsNotClosed => "auth.sessions_not_closed",
        }
    }
}

/// The closed list of codes for `/login?error=<code>` (ADR-0008, amendment 2026-10-28): the
/// errors of the OAuth callback and a logout that could not revoke. A subset of
/// [`AuthErrorCode`] with the same strings; it is the only type the server writes into that
/// `Location`, so nothing from the request or the provider is ever reflected. Not part of
/// [`ErrorCode`]: it never appears in a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum CallbackErrorCode {
    #[serde(rename = "auth.invalid_state")]
    InvalidState,
    #[serde(rename = "auth.banned")]
    Banned,
    #[serde(rename = "auth.provider_error")]
    ProviderError,
    #[serde(rename = "auth.try_again")]
    TryAgain,
    #[serde(rename = "auth.already_linked")]
    AlreadyLinked,
    #[serde(rename = "auth.provider_already_linked")]
    ProviderAlreadyLinked,
    /// 009 RF8, RF9: a link without a recent authentication that counts.
    #[serde(rename = "auth.reauth_required")]
    ReauthRequired,
    /// 009 RF11, RF12.
    #[serde(rename = "auth.reauth_mismatch")]
    ReauthMismatch,
    /// `logout-all` without a valid session closed no other session (TA-37).
    #[serde(rename = "auth.sessions_not_closed")]
    SessionsNotClosed,
}

impl CallbackErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[
        Self::InvalidState,
        Self::Banned,
        Self::ProviderError,
        Self::TryAgain,
        Self::AlreadyLinked,
        Self::ProviderAlreadyLinked,
        Self::ReauthRequired,
        Self::ReauthMismatch,
        Self::SessionsNotClosed,
    ];

    /// The serialized code, equal to the [`AuthErrorCode`] it maps to.
    pub const fn as_str(self) -> &'static str {
        AuthErrorCode::from_callback(self).as_str()
    }
}

impl AuthErrorCode {
    const fn from_callback(code: CallbackErrorCode) -> Self {
        match code {
            CallbackErrorCode::InvalidState => Self::InvalidState,
            CallbackErrorCode::Banned => Self::Banned,
            CallbackErrorCode::ProviderError => Self::ProviderError,
            CallbackErrorCode::TryAgain => Self::TryAgain,
            CallbackErrorCode::AlreadyLinked => Self::AlreadyLinked,
            CallbackErrorCode::ProviderAlreadyLinked => Self::ProviderAlreadyLinked,
            CallbackErrorCode::ReauthRequired => Self::ReauthRequired,
            CallbackErrorCode::ReauthMismatch => Self::ReauthMismatch,
            CallbackErrorCode::SessionsNotClosed => Self::SessionsNotClosed,
        }
    }
}

impl From<CallbackErrorCode> for AuthErrorCode {
    fn from(code: CallbackErrorCode) -> Self {
        Self::from_callback(code)
    }
}

/// Profile codes (`profile.*`, 008 RF-C4): the answers of a rename.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ProfileErrorCode {
    #[serde(rename = "profile.display_name_invalid")]
    DisplayNameInvalid,
    #[serde(rename = "profile.display_name_reserved")]
    DisplayNameReserved,
    #[serde(rename = "profile.display_name_taken")]
    DisplayNameTaken,
}

impl ProfileErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[
        Self::DisplayNameInvalid,
        Self::DisplayNameReserved,
        Self::DisplayNameTaken,
    ];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "profile.";

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DisplayNameInvalid => "profile.display_name_invalid",
            Self::DisplayNameReserved => "profile.display_name_reserved",
            Self::DisplayNameTaken => "profile.display_name_taken",
        }
    }
}

/// Fight codes (`fight.*`, 010 RF36): the answers of the read API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum FightErrorCode {
    /// An id that is not a canonical decimal from 1 to `i32::MAX`, or a fight that does not exist.
    #[serde(rename = "fight.not_found")]
    NotFound,
    /// The database failed (RF34).
    #[serde(rename = "fight.unavailable")]
    Unavailable,
}

impl FightErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[Self::NotFound, Self::Unavailable];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "fight.";

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "fight.not_found",
            Self::Unavailable => "fight.unavailable",
        }
    }
}

/// Any error code the server can put in an HTTP body or a WebSocket message. Untagged, so in
/// TypeScript it is the union of the domain enums. Each enum owns one prefix and their string
/// sets never overlap, which keeps deserialization unambiguous. A new domain gets a new enum
/// and a new variant here, never codes inside an existing enum.
/// Betting codes (`bet.*`, feature 012 RF25, RF33, RF47).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum BetErrorCode {
    /// A body that does not parse, a `client_key` that is not a canonical UUID or a query out of shape (012 RF25, RF47): never serde's text.
    #[serde(rename = "bet.invalid_request")]
    InvalidRequest,
    /// The same `client_key` with another body (012 RF25, row 3).
    #[serde(rename = "bet.key_reused")]
    KeyReused,
    /// The window is not open, or the fight does not exist: never says which (010 RF29).
    #[serde(rename = "bet.window_closed")]
    WindowClosed,
    #[serde(rename = "bet.below_minimum")]
    BelowMinimum,
    /// The user already bet on the other side of this fight (012 RF26).
    #[serde(rename = "bet.side_locked")]
    SideLocked,
    #[serde(rename = "bet.too_many_bets")]
    TooManyBets,
    #[serde(rename = "bet.pool_full")]
    PoolFull,
    #[serde(rename = "bet.fight_cap_exceeded")]
    FightCapExceeded,
    #[serde(rename = "bet.insufficient_balance")]
    InsufficientBalance,
    #[serde(rename = "bet.unavailable")]
    Unavailable,
}

impl BetErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[
        Self::InvalidRequest,
        Self::KeyReused,
        Self::WindowClosed,
        Self::BelowMinimum,
        Self::SideLocked,
        Self::TooManyBets,
        Self::PoolFull,
        Self::FightCapExceeded,
        Self::InsufficientBalance,
        Self::Unavailable,
    ];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "bet.";

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "bet.invalid_request",
            Self::KeyReused => "bet.key_reused",
            Self::WindowClosed => "bet.window_closed",
            Self::BelowMinimum => "bet.below_minimum",
            Self::SideLocked => "bet.side_locked",
            Self::TooManyBets => "bet.too_many_bets",
            Self::PoolFull => "bet.pool_full",
            Self::FightCapExceeded => "bet.fight_cap_exceeded",
            Self::InsufficientBalance => "bet.insufficient_balance",
            Self::Unavailable => "bet.unavailable",
        }
    }
}

/// Daily grant codes (`bonus.*`, feature 012 RF29).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum BonusErrorCode {
    #[serde(rename = "bonus.already_claimed")]
    AlreadyClaimed,
}

impl BonusErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[Self::AlreadyClaimed];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "bonus.";

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyClaimed => "bonus.already_claimed",
        }
    }
}

/// Bailout codes (`bailout.*`, feature 012 RF30).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum BailoutErrorCode {
    /// Today's grant is still unclaimed: it comes first (012 RF30).
    #[serde(rename = "bailout.grant_available")]
    GrantAvailable,
    /// Balance plus open stakes reach the minimum bet (012 RF30).
    #[serde(rename = "bailout.not_eligible")]
    NotEligible,
    #[serde(rename = "bailout.already_claimed")]
    AlreadyClaimed,
}

impl BailoutErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[
        Self::GrantAvailable,
        Self::NotEligible,
        Self::AlreadyClaimed,
    ];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "bailout.";

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GrantAvailable => "bailout.grant_available",
            Self::NotEligible => "bailout.not_eligible",
            Self::AlreadyClaimed => "bailout.already_claimed",
        }
    }
}

/// Wallet codes (`wallet.*`, feature 012 RF29, RF30, RF32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum WalletErrorCode {
    #[serde(rename = "wallet.unavailable")]
    Unavailable,
}

impl WalletErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[Self::Unavailable];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "wallet.";

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "wallet.unavailable",
        }
    }
}

/// Season codes (`season.*`, feature 020).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum SeasonErrorCode {
    /// A `season` that is not `YYYYMM`, or a season without a row.
    #[serde(rename = "season.not_found")]
    NotFound,
    /// The database failed.
    #[serde(rename = "season.unavailable")]
    Unavailable,
}

impl SeasonErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[Self::NotFound, Self::Unavailable];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "season.";

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "season.not_found",
            Self::Unavailable => "season.unavailable",
        }
    }
}

/// Botarga codes (`botarga.*`, feature 021).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum BotargaErrorCode {
    /// The user has no botarga yet: `POST /api/botarga` creates it.
    #[serde(rename = "botarga.not_created")]
    NotCreated,
    /// A body that is not a build: malformed, an unknown field, a wrong type, over the size limit,
    /// an id that is too long or too many ids.
    #[serde(rename = "botarga.invalid_request")]
    InvalidRequest,
    /// An ability, an accessory or a cosmetic the user does not own, or that the catalog does not
    /// have: the same code, so the answer says nothing about the catalog.
    #[serde(rename = "botarga.not_unlocked")]
    NotUnlocked,
    /// A fighting style the catalog does not have.
    #[serde(rename = "botarga.style_unknown")]
    StyleUnknown,
    /// The stats add up to more than the budget of the level.
    #[serde(rename = "botarga.budget_exceeded")]
    BudgetExceeded,
    /// A stat, after the accessories, outside the range of the ruleset.
    #[serde(rename = "botarga.stat_out_of_range")]
    StatOutOfRange,
    /// More abilities than the slots open at the level.
    #[serde(rename = "botarga.slots_locked")]
    SlotsLocked,
    /// No ability at all, or more than the ruleset allows.
    #[serde(rename = "botarga.ability_count")]
    AbilityCount,
    /// Too many heavies or too many specials.
    #[serde(rename = "botarga.ability_kind_count")]
    AbilityKindCount,
    /// The same ability twice.
    #[serde(rename = "botarga.ability_duplicate")]
    AbilityDuplicate,
    /// The elements of the abilities bring too many weaknesses.
    #[serde(rename = "botarga.weakness_count")]
    WeaknessCount,
    /// Two accessories in the same slot.
    #[serde(rename = "botarga.accessory_slot_duplicate")]
    AccessorySlotDuplicate,
    /// The accessories together move a stat beyond the cap.
    #[serde(rename = "botarga.accessory_stat_total")]
    AccessoryStatTotal,
    /// Any other rule of the loadout or of the ruleset.
    #[serde(rename = "botarga.build_invalid")]
    BuildInvalid,
    /// The database failed, or the row is locked.
    #[serde(rename = "botarga.unavailable")]
    Unavailable,
}

impl BotargaErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[
        Self::NotCreated,
        Self::InvalidRequest,
        Self::NotUnlocked,
        Self::StyleUnknown,
        Self::BudgetExceeded,
        Self::StatOutOfRange,
        Self::SlotsLocked,
        Self::AbilityCount,
        Self::AbilityKindCount,
        Self::AbilityDuplicate,
        Self::WeaknessCount,
        Self::AccessorySlotDuplicate,
        Self::AccessoryStatTotal,
        Self::BuildInvalid,
        Self::Unavailable,
    ];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "botarga.";

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotCreated => "botarga.not_created",
            Self::InvalidRequest => "botarga.invalid_request",
            Self::NotUnlocked => "botarga.not_unlocked",
            Self::StyleUnknown => "botarga.style_unknown",
            Self::BudgetExceeded => "botarga.budget_exceeded",
            Self::StatOutOfRange => "botarga.stat_out_of_range",
            Self::SlotsLocked => "botarga.slots_locked",
            Self::AbilityCount => "botarga.ability_count",
            Self::AbilityKindCount => "botarga.ability_kind_count",
            Self::AbilityDuplicate => "botarga.ability_duplicate",
            Self::WeaknessCount => "botarga.weakness_count",
            Self::AccessorySlotDuplicate => "botarga.accessory_slot_duplicate",
            Self::AccessoryStatTotal => "botarga.accessory_stat_total",
            Self::BuildInvalid => "botarga.build_invalid",
            Self::Unavailable => "botarga.unavailable",
        }
    }
}

/// Player fight codes (`pvp.*`, feature 021).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PvpErrorCode {
    /// A body that is not a search: malformed, an unknown field, or a `client_key` that is not a
    /// canonical UUID; or a query that is not a fight id.
    #[serde(rename = "pvp.invalid_request")]
    InvalidRequest,
    /// No player fight with that id.
    #[serde(rename = "pvp.not_found")]
    NotFound,
    /// The saved build of the user no longer fits the catalog of this release: saving it again
    /// says why.
    #[serde(rename = "pvp.build_invalid")]
    BuildInvalid,
    /// There is no other botarga nor any character of the tournament to fight.
    #[serde(rename = "pvp.no_rival")]
    NoRival,
    /// A row was locked for more than the `lock_timeout`: the client retries with the same key.
    #[serde(rename = "pvp.busy")]
    Busy,
    /// The database failed.
    #[serde(rename = "pvp.unavailable")]
    Unavailable,
}

impl PvpErrorCode {
    /// Every variant. A test with an exhaustive `match` keeps this list complete.
    pub const ALL: &'static [Self] = &[
        Self::InvalidRequest,
        Self::NotFound,
        Self::BuildInvalid,
        Self::NoRival,
        Self::Busy,
        Self::Unavailable,
    ];
    /// The only domain prefix this enum may use.
    pub const PREFIX: &'static str = "pvp.";

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "pvp.invalid_request",
            Self::NotFound => "pvp.not_found",
            Self::BuildInvalid => "pvp.build_invalid",
            Self::NoRival => "pvp.no_rival",
            Self::Busy => "pvp.busy",
            Self::Unavailable => "pvp.unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(untagged)]
pub enum ErrorCode {
    Http(ApiErrorCode),
    Ws(WsErrorCode),
    Auth(AuthErrorCode),
    Profile(ProfileErrorCode),
    Fight(FightErrorCode),
    Bet(BetErrorCode),
    Bonus(BonusErrorCode),
    Bailout(BailoutErrorCode),
    Wallet(WalletErrorCode),
    Season(SeasonErrorCode),
    Botarga(BotargaErrorCode),
    Pvp(PvpErrorCode),
}

impl ErrorCode {
    /// The serialized code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http(code) => code.as_str(),
            Self::Ws(code) => code.as_str(),
            Self::Auth(code) => code.as_str(),
            Self::Profile(code) => code.as_str(),
            Self::Fight(code) => code.as_str(),
            Self::Bet(code) => code.as_str(),
            Self::Bonus(code) => code.as_str(),
            Self::Bailout(code) => code.as_str(),
            Self::Wallet(code) => code.as_str(),
            Self::Season(code) => code.as_str(),
            Self::Botarga(code) => code.as_str(),
            Self::Pvp(code) => code.as_str(),
        }
    }
}

impl From<ApiErrorCode> for ErrorCode {
    fn from(code: ApiErrorCode) -> Self {
        Self::Http(code)
    }
}

impl From<WsErrorCode> for ErrorCode {
    fn from(code: WsErrorCode) -> Self {
        Self::Ws(code)
    }
}

impl From<AuthErrorCode> for ErrorCode {
    fn from(code: AuthErrorCode) -> Self {
        Self::Auth(code)
    }
}

impl From<ProfileErrorCode> for ErrorCode {
    fn from(code: ProfileErrorCode) -> Self {
        Self::Profile(code)
    }
}

impl From<FightErrorCode> for ErrorCode {
    fn from(code: FightErrorCode) -> Self {
        Self::Fight(code)
    }
}

impl From<BetErrorCode> for ErrorCode {
    fn from(code: BetErrorCode) -> Self {
        Self::Bet(code)
    }
}

impl From<BonusErrorCode> for ErrorCode {
    fn from(code: BonusErrorCode) -> Self {
        Self::Bonus(code)
    }
}

impl From<SeasonErrorCode> for ErrorCode {
    fn from(code: SeasonErrorCode) -> Self {
        Self::Season(code)
    }
}

impl From<BotargaErrorCode> for ErrorCode {
    fn from(code: BotargaErrorCode) -> Self {
        Self::Botarga(code)
    }
}

impl From<PvpErrorCode> for ErrorCode {
    fn from(code: PvpErrorCode) -> Self {
        Self::Pvp(code)
    }
}

impl From<BailoutErrorCode> for ErrorCode {
    fn from(code: BailoutErrorCode) -> Self {
        Self::Bailout(code)
    }
}

impl From<WalletErrorCode> for ErrorCode {
    fn from(code: WalletErrorCode) -> Self {
        Self::Wallet(code)
    }
}

/// Body of every HTTP error response of the server (RF-B5): `{"error":"<code>"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ApiError {
    pub error: ErrorCode,
}

impl ApiError {
    pub fn new(code: impl Into<ErrorCode>) -> Self {
        Self { error: code.into() }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Fails to compile when a variant is added without listing it in `ALL`'s test below.
    fn api_index(code: ApiErrorCode) -> usize {
        match code {
            ApiErrorCode::NotFound => 0,
            ApiErrorCode::MethodNotAllowed => 1,
            ApiErrorCode::RateLimited => 2,
            ApiErrorCode::InvalidBody => 3,
        }
    }

    fn ws_index(code: WsErrorCode) -> usize {
        match code {
            WsErrorCode::OriginForbidden => 0,
            WsErrorCode::Capacity => 1,
            WsErrorCode::InvalidMessage => 2,
            WsErrorCode::EchoDisabled => 3,
            WsErrorCode::RateLimited => 4,
            WsErrorCode::PongTimeout => 5,
            WsErrorCode::IdleTimeout => 6,
            WsErrorCode::IpCapacity => 7,
            WsErrorCode::UserCapacity => 8,
            WsErrorCode::ForbiddenChannel => 9,
            WsErrorCode::TooManySubscriptions => 10,
        }
    }

    fn auth_index(code: AuthErrorCode) -> usize {
        match code {
            AuthErrorCode::CsrfOrigin => 0,
            AuthErrorCode::InvalidState => 1,
            AuthErrorCode::Banned => 2,
            AuthErrorCode::ProviderError => 3,
            AuthErrorCode::TryAgain => 4,
            AuthErrorCode::Unauthenticated => 5,
            AuthErrorCode::AlreadyLinked => 6,
            AuthErrorCode::ProviderAlreadyLinked => 7,
            AuthErrorCode::LastProvider => 8,
            AuthErrorCode::ReauthRequired => 9,
            AuthErrorCode::ReauthMismatch => 10,
            AuthErrorCode::SessionsNotClosed => 11,
        }
    }

    fn callback_index(code: CallbackErrorCode) -> usize {
        match code {
            CallbackErrorCode::InvalidState => 0,
            CallbackErrorCode::Banned => 1,
            CallbackErrorCode::ProviderError => 2,
            CallbackErrorCode::TryAgain => 3,
            CallbackErrorCode::AlreadyLinked => 4,
            CallbackErrorCode::ProviderAlreadyLinked => 5,
            CallbackErrorCode::ReauthRequired => 6,
            CallbackErrorCode::ReauthMismatch => 7,
            CallbackErrorCode::SessionsNotClosed => 8,
        }
    }

    fn profile_index(code: ProfileErrorCode) -> usize {
        match code {
            ProfileErrorCode::DisplayNameInvalid => 0,
            ProfileErrorCode::DisplayNameReserved => 1,
            ProfileErrorCode::DisplayNameTaken => 2,
        }
    }

    fn fight_index(code: FightErrorCode) -> usize {
        match code {
            FightErrorCode::NotFound => 0,
            FightErrorCode::Unavailable => 1,
        }
    }

    fn season_index(code: SeasonErrorCode) -> usize {
        match code {
            SeasonErrorCode::NotFound => 0,
            SeasonErrorCode::Unavailable => 1,
        }
    }

    fn botarga_index(code: BotargaErrorCode) -> usize {
        match code {
            BotargaErrorCode::NotCreated => 0,
            BotargaErrorCode::InvalidRequest => 1,
            BotargaErrorCode::NotUnlocked => 2,
            BotargaErrorCode::StyleUnknown => 3,
            BotargaErrorCode::BudgetExceeded => 4,
            BotargaErrorCode::StatOutOfRange => 5,
            BotargaErrorCode::SlotsLocked => 6,
            BotargaErrorCode::AbilityCount => 7,
            BotargaErrorCode::AbilityKindCount => 8,
            BotargaErrorCode::AbilityDuplicate => 9,
            BotargaErrorCode::WeaknessCount => 10,
            BotargaErrorCode::AccessorySlotDuplicate => 11,
            BotargaErrorCode::AccessoryStatTotal => 12,
            BotargaErrorCode::BuildInvalid => 13,
            BotargaErrorCode::Unavailable => 14,
        }
    }

    fn pvp_index(code: PvpErrorCode) -> usize {
        match code {
            PvpErrorCode::InvalidRequest => 0,
            PvpErrorCode::NotFound => 1,
            PvpErrorCode::BuildInvalid => 2,
            PvpErrorCode::NoRival => 3,
            PvpErrorCode::Busy => 4,
            PvpErrorCode::Unavailable => 5,
        }
    }

    fn bet_index(code: BetErrorCode) -> usize {
        match code {
            BetErrorCode::InvalidRequest => 0,
            BetErrorCode::KeyReused => 1,
            BetErrorCode::WindowClosed => 2,
            BetErrorCode::BelowMinimum => 3,
            BetErrorCode::SideLocked => 4,
            BetErrorCode::TooManyBets => 5,
            BetErrorCode::PoolFull => 6,
            BetErrorCode::FightCapExceeded => 7,
            BetErrorCode::InsufficientBalance => 8,
            BetErrorCode::Unavailable => 9,
        }
    }

    fn bonus_index(code: BonusErrorCode) -> usize {
        match code {
            BonusErrorCode::AlreadyClaimed => 0,
        }
    }

    fn bailout_index(code: BailoutErrorCode) -> usize {
        match code {
            BailoutErrorCode::GrantAvailable => 0,
            BailoutErrorCode::NotEligible => 1,
            BailoutErrorCode::AlreadyClaimed => 2,
        }
    }

    fn wallet_index(code: WalletErrorCode) -> usize {
        match code {
            WalletErrorCode::Unavailable => 0,
        }
    }

    const API_COUNT: usize = 4;
    const WS_COUNT: usize = 11;
    const AUTH_COUNT: usize = 12;
    const CALLBACK_COUNT: usize = 9;
    const PROFILE_COUNT: usize = 3;
    const FIGHT_COUNT: usize = 2;
    const SEASON_COUNT: usize = 2;
    const BOTARGA_COUNT: usize = 15;
    const PVP_COUNT: usize = 6;
    const BET_COUNT: usize = 10;
    const BONUS_COUNT: usize = 1;
    const BAILOUT_COUNT: usize = 3;
    const WALLET_COUNT: usize = 1;

    #[test]
    fn all_lists_every_variant_once() {
        let api: HashSet<usize> = ApiErrorCode::ALL.iter().map(|c| api_index(*c)).collect();
        assert_eq!(api.len(), API_COUNT);
        assert_eq!(ApiErrorCode::ALL.len(), API_COUNT);
        let ws: HashSet<usize> = WsErrorCode::ALL.iter().map(|c| ws_index(*c)).collect();
        assert_eq!(ws.len(), WS_COUNT);
        assert_eq!(WsErrorCode::ALL.len(), WS_COUNT);
        let auth: HashSet<usize> = AuthErrorCode::ALL.iter().map(|c| auth_index(*c)).collect();
        assert_eq!(auth.len(), AUTH_COUNT);
        assert_eq!(AuthErrorCode::ALL.len(), AUTH_COUNT);
        let callback: HashSet<usize> = CallbackErrorCode::ALL
            .iter()
            .map(|c| callback_index(*c))
            .collect();
        assert_eq!(callback.len(), CALLBACK_COUNT);
        assert_eq!(CallbackErrorCode::ALL.len(), CALLBACK_COUNT);
        let profile: HashSet<usize> = ProfileErrorCode::ALL
            .iter()
            .map(|c| profile_index(*c))
            .collect();
        assert_eq!(profile.len(), PROFILE_COUNT);
        assert_eq!(ProfileErrorCode::ALL.len(), PROFILE_COUNT);
        let fight: HashSet<usize> = FightErrorCode::ALL
            .iter()
            .map(|c| fight_index(*c))
            .collect();
        assert_eq!(fight.len(), FIGHT_COUNT);
        assert_eq!(FightErrorCode::ALL.len(), FIGHT_COUNT);
        let season: HashSet<usize> = SeasonErrorCode::ALL
            .iter()
            .map(|c| season_index(*c))
            .collect();
        assert_eq!(season.len(), SEASON_COUNT);
        assert_eq!(SeasonErrorCode::ALL.len(), SEASON_COUNT);
        let botarga: HashSet<usize> = BotargaErrorCode::ALL
            .iter()
            .map(|c| botarga_index(*c))
            .collect();
        assert_eq!(botarga.len(), BOTARGA_COUNT);
        assert_eq!(BotargaErrorCode::ALL.len(), BOTARGA_COUNT);
        let pvp: HashSet<usize> = PvpErrorCode::ALL.iter().map(|c| pvp_index(*c)).collect();
        assert_eq!(pvp.len(), PVP_COUNT);
        assert_eq!(PvpErrorCode::ALL.len(), PVP_COUNT);
    }

    /// The codes of 012 RF38, each wrapped in its `ErrorCode` variant.
    fn money_codes() -> Vec<ErrorCode> {
        let mut codes: Vec<ErrorCode> = BetErrorCode::ALL.iter().map(|c| (*c).into()).collect();
        codes.extend(BonusErrorCode::ALL.iter().map(|c| ErrorCode::from(*c)));
        codes.extend(BailoutErrorCode::ALL.iter().map(|c| ErrorCode::from(*c)));
        codes.extend(WalletErrorCode::ALL.iter().map(|c| ErrorCode::from(*c)));
        codes
    }

    #[test]
    fn money_codes_list_every_variant_once() {
        let bet: HashSet<usize> = BetErrorCode::ALL.iter().map(|c| bet_index(*c)).collect();
        assert_eq!((bet.len(), BetErrorCode::ALL.len()), (BET_COUNT, BET_COUNT));
        let bonus: HashSet<usize> = BonusErrorCode::ALL
            .iter()
            .map(|c| bonus_index(*c))
            .collect();
        assert_eq!(
            (bonus.len(), BonusErrorCode::ALL.len()),
            (BONUS_COUNT, BONUS_COUNT)
        );
        let bailout: HashSet<usize> = BailoutErrorCode::ALL
            .iter()
            .map(|c| bailout_index(*c))
            .collect();
        assert_eq!(
            (bailout.len(), BailoutErrorCode::ALL.len()),
            (BAILOUT_COUNT, BAILOUT_COUNT)
        );
        let wallet: HashSet<usize> = WalletErrorCode::ALL
            .iter()
            .map(|c| wallet_index(*c))
            .collect();
        assert_eq!(
            (wallet.len(), WalletErrorCode::ALL.len()),
            (WALLET_COUNT, WALLET_COUNT)
        );
    }

    /// 012 RF38: the exact strings, and each one comes back as its own `ErrorCode` variant.
    #[test]
    fn money_codes_have_the_exact_strings_and_round_trip() {
        let expected = [
            (
                ErrorCode::Bet(BetErrorCode::InvalidRequest),
                "bet.invalid_request",
            ),
            (ErrorCode::Bet(BetErrorCode::KeyReused), "bet.key_reused"),
            (
                ErrorCode::Bet(BetErrorCode::WindowClosed),
                "bet.window_closed",
            ),
            (
                ErrorCode::Bet(BetErrorCode::BelowMinimum),
                "bet.below_minimum",
            ),
            (ErrorCode::Bet(BetErrorCode::SideLocked), "bet.side_locked"),
            (
                ErrorCode::Bet(BetErrorCode::TooManyBets),
                "bet.too_many_bets",
            ),
            (ErrorCode::Bet(BetErrorCode::PoolFull), "bet.pool_full"),
            (
                ErrorCode::Bet(BetErrorCode::FightCapExceeded),
                "bet.fight_cap_exceeded",
            ),
            (
                ErrorCode::Bet(BetErrorCode::InsufficientBalance),
                "bet.insufficient_balance",
            ),
            (ErrorCode::Bet(BetErrorCode::Unavailable), "bet.unavailable"),
            (
                ErrorCode::Bonus(BonusErrorCode::AlreadyClaimed),
                "bonus.already_claimed",
            ),
            (
                ErrorCode::Bailout(BailoutErrorCode::GrantAvailable),
                "bailout.grant_available",
            ),
            (
                ErrorCode::Bailout(BailoutErrorCode::NotEligible),
                "bailout.not_eligible",
            ),
            (
                ErrorCode::Bailout(BailoutErrorCode::AlreadyClaimed),
                "bailout.already_claimed",
            ),
            (
                ErrorCode::Wallet(WalletErrorCode::Unavailable),
                "wallet.unavailable",
            ),
        ];
        assert_eq!(expected.len(), money_codes().len());
        for (code, text) in expected {
            assert_eq!(code.as_str(), text);
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("\"{text}\""));
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&format!("\"{text}\"")).unwrap(),
                code
            );
        }
    }

    #[test]
    fn callback_codes_are_exactly_auth_codes_with_the_same_strings() {
        let expected = [
            (CallbackErrorCode::InvalidState, "auth.invalid_state"),
            (CallbackErrorCode::Banned, "auth.banned"),
            (CallbackErrorCode::ProviderError, "auth.provider_error"),
            (CallbackErrorCode::TryAgain, "auth.try_again"),
            (CallbackErrorCode::AlreadyLinked, "auth.already_linked"),
            (
                CallbackErrorCode::ProviderAlreadyLinked,
                "auth.provider_already_linked",
            ),
            // 009 RF19: the closed list grows from six to eight.
            (CallbackErrorCode::ReauthRequired, "auth.reauth_required"),
            (CallbackErrorCode::ReauthMismatch, "auth.reauth_mismatch"),
            // TA-37: the ninth.
            (
                CallbackErrorCode::SessionsNotClosed,
                "auth.sessions_not_closed",
            ),
        ];
        assert_eq!(expected.len(), CALLBACK_COUNT);
        for (code, text) in expected {
            assert_eq!(code.as_str(), text);
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("\"{text}\""));
            assert_eq!(AuthErrorCode::from(code).as_str(), text);
            assert_eq!(
                serde_json::from_str::<CallbackErrorCode>(&format!("\"{text}\"")).unwrap(),
                code
            );
        }
        // Not in the closed list: never a callback error.
        for text in [
            "auth.csrf_origin",
            "auth.unauthenticated",
            "auth.last_provider",
            "http.not_found",
            "profile.display_name_taken",
        ] {
            assert!(serde_json::from_str::<CallbackErrorCode>(&format!("\"{text}\"")).is_err());
        }
    }

    #[test]
    fn api_codes_have_the_exact_strings() {
        let expected = [
            (ApiErrorCode::NotFound, "http.not_found"),
            (ApiErrorCode::MethodNotAllowed, "http.method_not_allowed"),
            (ApiErrorCode::RateLimited, "http.rate_limited"),
            (ApiErrorCode::InvalidBody, "http.invalid_body"),
        ];
        assert_eq!(expected.len(), API_COUNT);
        for (code, text) in expected {
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("\"{text}\""));
            assert_eq!(
                serde_json::from_str::<ApiErrorCode>(&format!("\"{text}\"")).unwrap(),
                code
            );
        }
    }

    #[test]
    fn ws_codes_have_the_exact_strings() {
        let expected = [
            (WsErrorCode::OriginForbidden, "ws.origin_forbidden"),
            (WsErrorCode::Capacity, "ws.capacity"),
            (WsErrorCode::InvalidMessage, "ws.invalid_message"),
            (WsErrorCode::EchoDisabled, "ws.echo_disabled"),
            (WsErrorCode::RateLimited, "ws.rate_limited"),
            (WsErrorCode::PongTimeout, "ws.pong_timeout"),
            (WsErrorCode::IdleTimeout, "ws.idle_timeout"),
            (WsErrorCode::IpCapacity, "ws.ip_capacity"),
            (WsErrorCode::UserCapacity, "ws.user_capacity"),
            (WsErrorCode::ForbiddenChannel, "ws.forbidden_channel"),
            (
                WsErrorCode::TooManySubscriptions,
                "ws.too_many_subscriptions",
            ),
        ];
        assert_eq!(expected.len(), WS_COUNT);
        for (code, text) in expected {
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("\"{text}\""));
            assert_eq!(
                serde_json::from_str::<WsErrorCode>(&format!("\"{text}\"")).unwrap(),
                code
            );
        }
    }

    #[test]
    fn auth_codes_have_the_exact_strings() {
        let expected = [
            (AuthErrorCode::CsrfOrigin, "auth.csrf_origin"),
            (AuthErrorCode::InvalidState, "auth.invalid_state"),
            (AuthErrorCode::Banned, "auth.banned"),
            (AuthErrorCode::ProviderError, "auth.provider_error"),
            (AuthErrorCode::TryAgain, "auth.try_again"),
            (AuthErrorCode::Unauthenticated, "auth.unauthenticated"),
            (AuthErrorCode::AlreadyLinked, "auth.already_linked"),
            (
                AuthErrorCode::ProviderAlreadyLinked,
                "auth.provider_already_linked",
            ),
            (AuthErrorCode::LastProvider, "auth.last_provider"),
            (AuthErrorCode::ReauthRequired, "auth.reauth_required"),
            (AuthErrorCode::ReauthMismatch, "auth.reauth_mismatch"),
            (AuthErrorCode::SessionsNotClosed, "auth.sessions_not_closed"),
        ];
        assert_eq!(expected.len(), AUTH_COUNT);
        for (code, text) in expected {
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("\"{text}\""));
            assert_eq!(
                serde_json::from_str::<AuthErrorCode>(&format!("\"{text}\"")).unwrap(),
                code
            );
        }
    }

    #[test]
    fn profile_codes_have_the_exact_strings() {
        let expected = [
            (
                ProfileErrorCode::DisplayNameInvalid,
                "profile.display_name_invalid",
            ),
            (
                ProfileErrorCode::DisplayNameReserved,
                "profile.display_name_reserved",
            ),
            (
                ProfileErrorCode::DisplayNameTaken,
                "profile.display_name_taken",
            ),
        ];
        assert_eq!(expected.len(), PROFILE_COUNT);
        for (code, text) in expected {
            assert_eq!(code.as_str(), text);
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("\"{text}\""));
            assert_eq!(
                serde_json::from_str::<ProfileErrorCode>(&format!("\"{text}\"")).unwrap(),
                code
            );
        }
    }

    #[test]
    fn as_str_matches_serde_for_every_code() {
        for code in ApiErrorCode::ALL {
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::Value::from(code.as_str())
            );
            assert_eq!(ErrorCode::from(*code).as_str(), code.as_str());
        }
        for code in WsErrorCode::ALL {
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::Value::from(code.as_str())
            );
            assert_eq!(ErrorCode::from(*code).as_str(), code.as_str());
        }
        for code in AuthErrorCode::ALL {
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::Value::from(code.as_str())
            );
            assert_eq!(ErrorCode::from(*code).as_str(), code.as_str());
        }
        for code in ProfileErrorCode::ALL {
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::Value::from(code.as_str())
            );
            assert_eq!(ErrorCode::from(*code).as_str(), code.as_str());
        }
        for code in FightErrorCode::ALL {
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::Value::from(code.as_str())
            );
            assert_eq!(ErrorCode::from(*code).as_str(), code.as_str());
        }
    }

    #[test]
    fn code_sets_use_their_prefix_and_do_not_overlap() {
        let mut seen = HashSet::new();
        for code in ApiErrorCode::ALL {
            assert!(code.as_str().starts_with(ApiErrorCode::PREFIX), "{code:?}");
            assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        }
        for code in WsErrorCode::ALL {
            assert!(code.as_str().starts_with(WsErrorCode::PREFIX), "{code:?}");
            assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        }
        for code in AuthErrorCode::ALL {
            assert!(code.as_str().starts_with(AuthErrorCode::PREFIX), "{code:?}");
            assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        }
        for code in ProfileErrorCode::ALL {
            assert!(
                code.as_str().starts_with(ProfileErrorCode::PREFIX),
                "{code:?}"
            );
            assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        }
        for code in FightErrorCode::ALL {
            assert!(
                code.as_str().starts_with(FightErrorCode::PREFIX),
                "{code:?}"
            );
            assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        }
        for code in SeasonErrorCode::ALL {
            assert!(
                code.as_str().starts_with(SeasonErrorCode::PREFIX),
                "{code:?}"
            );
            assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        }
        for code in BotargaErrorCode::ALL {
            assert!(
                code.as_str().starts_with(BotargaErrorCode::PREFIX),
                "{code:?}"
            );
            assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        }
        for code in PvpErrorCode::ALL {
            assert!(code.as_str().starts_with(PvpErrorCode::PREFIX), "{code:?}");
            assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        }
        for code in money_codes() {
            let prefix = match code {
                ErrorCode::Bet(_) => BetErrorCode::PREFIX,
                ErrorCode::Bonus(_) => BonusErrorCode::PREFIX,
                ErrorCode::Bailout(_) => BailoutErrorCode::PREFIX,
                ErrorCode::Wallet(_) => WalletErrorCode::PREFIX,
                other => panic!("not a money code: {other:?}"),
            };
            assert!(code.as_str().starts_with(prefix), "{code:?}");
            assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        }
        assert_eq!(
            seen.len(),
            API_COUNT
                + WS_COUNT
                + AUTH_COUNT
                + PROFILE_COUNT
                + FIGHT_COUNT
                + SEASON_COUNT
                + BOTARGA_COUNT
                + PVP_COUNT
                + BET_COUNT
                + BONUS_COUNT
                + BAILOUT_COUNT
                + WALLET_COUNT
        );
    }

    #[test]
    fn every_code_round_trips_to_the_right_error_code_variant() {
        for code in ApiErrorCode::ALL {
            let json = format!("\"{}\"", code.as_str());
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&json).unwrap(),
                ErrorCode::Http(*code)
            );
        }
        for code in WsErrorCode::ALL {
            let json = format!("\"{}\"", code.as_str());
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&json).unwrap(),
                ErrorCode::Ws(*code)
            );
        }
        for code in AuthErrorCode::ALL {
            let json = format!("\"{}\"", code.as_str());
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&json).unwrap(),
                ErrorCode::Auth(*code)
            );
        }
        for code in ProfileErrorCode::ALL {
            let json = format!("\"{}\"", code.as_str());
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&json).unwrap(),
                ErrorCode::Profile(*code)
            );
        }
        for code in FightErrorCode::ALL {
            let json = format!("\"{}\"", code.as_str());
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&json).unwrap(),
                ErrorCode::Fight(*code)
            );
        }
        for code in SeasonErrorCode::ALL {
            let json = format!("\"{}\"", code.as_str());
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&json).unwrap(),
                ErrorCode::Season(*code)
            );
        }
        for code in BotargaErrorCode::ALL {
            let json = format!("\"{}\"", code.as_str());
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&json).unwrap(),
                ErrorCode::Botarga(*code)
            );
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::Value::from(code.as_str())
            );
            assert_eq!(ErrorCode::from(*code).as_str(), code.as_str());
        }
        for code in PvpErrorCode::ALL {
            let json = format!("\"{}\"", code.as_str());
            assert_eq!(
                serde_json::from_str::<ErrorCode>(&json).unwrap(),
                ErrorCode::Pvp(*code)
            );
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::Value::from(code.as_str())
            );
            assert_eq!(ErrorCode::from(*code).as_str(), code.as_str());
        }
        assert_eq!(
            serde_json::from_str::<ErrorCode>("\"auth.csrf_origin\"").unwrap(),
            ErrorCode::Auth(AuthErrorCode::CsrfOrigin)
        );
        assert!(serde_json::from_str::<ErrorCode>("\"auth.unknown\"").is_err());
    }

    #[test]
    fn api_error_has_the_exact_shape() {
        let cases = [
            (
                ApiError::new(ApiErrorCode::NotFound),
                r#"{"error":"http.not_found"}"#,
            ),
            (
                ApiError::new(ApiErrorCode::MethodNotAllowed),
                r#"{"error":"http.method_not_allowed"}"#,
            ),
            (
                ApiError::new(WsErrorCode::OriginForbidden),
                r#"{"error":"ws.origin_forbidden"}"#,
            ),
            (
                ApiError::new(WsErrorCode::Capacity),
                r#"{"error":"ws.capacity"}"#,
            ),
            (
                ApiError::new(AuthErrorCode::CsrfOrigin),
                r#"{"error":"auth.csrf_origin"}"#,
            ),
            (
                ApiError::new(ApiErrorCode::RateLimited),
                r#"{"error":"http.rate_limited"}"#,
            ),
            (
                ApiError::new(ApiErrorCode::InvalidBody),
                r#"{"error":"http.invalid_body"}"#,
            ),
            (
                ApiError::new(ProfileErrorCode::DisplayNameTaken),
                r#"{"error":"profile.display_name_taken"}"#,
            ),
            (
                ApiError::new(AuthErrorCode::ReauthRequired),
                r#"{"error":"auth.reauth_required"}"#,
            ),
        ];
        for (value, json) in cases {
            assert_eq!(serde_json::to_string(&value).unwrap(), json);
            assert_eq!(serde_json::from_str::<ApiError>(json).unwrap(), value);
        }
    }
}
