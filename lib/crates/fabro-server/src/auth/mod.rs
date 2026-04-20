mod cli_flow;
mod github_endpoints;
mod jwt;
mod keys;

pub(crate) use cli_flow::{api_routes, web_routes};
#[allow(
    unused_imports,
    reason = "Auth internals are wired incrementally across implementation units."
)]
pub(crate) use fabro_store::{
    AuthCode, ConsumeOutcome, RefreshToken, SlateAuthCodeStore, SlateAuthTokenStore,
};
pub use github_endpoints::GithubEndpoints;
#[allow(
    unused_imports,
    reason = "Auth internals are wired incrementally across implementation units."
)]
pub(crate) use jwt::{Claims, JwtError, JwtSubject, issue, verify};
pub(crate) use keys::{JwtSigningKey, KeyDeriveError, derive_cookie_key, derive_jwt_key};
