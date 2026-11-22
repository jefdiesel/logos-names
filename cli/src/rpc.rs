//! The one door every JSON-RPC call to a sequencer goes through: the wallet's client
//! ([`Rpc`], handed to `WalletCore` in place of the bare jsonrpsee client) and the plain
//! ureq calls ([`post`]).
//!
//! A public RPC is rate-limited per IP (lez.nameswallet.app: a Cloudflare rule at 50 requests
//! in 10 s, and a proxy whose bucket refills 5 a second). It refuses with HTTP 429 (the proxy
//! adds JSON-RPC error -32005 and `Retry-After`). So every call here:
//! - takes a turn from a pacer shared by the whole process (4 a second, a burst of 8: under 50
//!   in any 10 s), except on this computer's own sequencer;
//! - on a 429 or a -32005, waits (the `Retry-After` seconds, else 2 s doubling, at most 10 s)
//!   and tries again, up to 4 times, then fails with [`BUSY`].

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use common::block::Block;
use common::transaction::LeeTransaction;
use common::{HashType, PageLimit};
use lee::{Account, AccountId, EncryptedNote, ProgramId};
use lee_core::account::{Actor, Nonce};
use lee_core::program::Publication;
use lee_core::{BlockId, Commitment, CommitmentPosition, CommitmentSetDigest, MembershipProof};
use jsonrpsee::core::client::{BatchResponse, ClientT, Error as ClientError};
use jsonrpsee::core::params::BatchRequestBuilder;
use jsonrpsee::core::traits::ToRpcParams;
use jsonrpsee::http_client::{HttpBackend, HttpClient, HttpClientBuilder, RpcLogger, RpcService};
use serde::de::DeserializeOwned;
use serde_json::value::RawValue;
use sequencer_service_rpc::RpcClient;

/// What a call says once the RPC has refused it every time.
pub const BUSY: &str = "the RPC is busy (rate limited); try again in a minute";

/// The JSON-RPC error code the RPC proxy refuses with when a caller goes too fast.
pub const RATE_LIMITED_CODE: i32 = -32005;

/// Tries after the first one.
pub const RETRIES: u32 = 4;

/// How long to wait before each try again.
#[derive(Debug, Clone, Copy)]
pub struct Backoff {
    /// Before the first try again, without a `Retry-After`; doubled each time after.
    pub first: Duration,
    /// The longest wait, `Retry-After` included.
    pub cap: Duration,
}

impl Backoff {
    pub const DEFAULT: Self = Self { first: Duration::from_secs(2), cap: Duration::from_secs(10) };

    /// The wait before try again number `retry` (0 for the first), given the `Retry-After`
    /// seconds the refusal carried, if any.
    pub fn wait(&self, retry: u32, retry_after: Option<u64>) -> Duration {
        match retry_after {
            Some(seconds) => Duration::from_secs(seconds).min(self.cap),
            None => self.first.saturating_mul(1u32 << retry.min(16)).min(self.cap),
        }
    }
}

/// The error a call ends with once the RPC refused it [`RETRIES`] + 1 times.
#[derive(Debug)]
pub struct Busy;

impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(BUSY)
    }
}

impl std::error::Error for Busy {}

/// Whether an error anywhere in `error`'s chain is [`BUSY`].
pub fn is_busy(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.to_string().contains(BUSY))
}

/// What `names` ends with: [`BUSY`] on top when that is why it stopped, so the wallet (which
/// shows the first `Error:` line) says it plainly. The step it was on stays below it, under
/// "Caused by", and an error that already says it on top (a "nothing is lost, run it again"
/// message) is left as it is.
pub fn plain_when_busy(error: anyhow::Error) -> anyhow::Error {
    if is_busy(&error) && !error.to_string().contains(BUSY) { error.context(BUSY) } else { error }
}

// ── Pacing ──

/// A token bucket, shared by every call this process makes to a remote sequencer.
struct Pacer {
    state: Mutex<(f64, Instant)>,
}

const PACE_PER_SECOND: f64 = 4.0;
const PACE_BURST: f64 = 8.0;

impl Pacer {
    /// Takes a turn and says how long to wait for it (zero while the burst lasts). A turn is
    /// reserved even when it has to be waited for, so callers queue in order.
    fn reserve(&self) -> Duration {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Instant::now();
        let (tokens, at) = *state;
        let tokens = (tokens + now.duration_since(at).as_secs_f64() * PACE_PER_SECOND).min(PACE_BURST) - 1.0;
        *state = (tokens, now);
        if tokens >= 0.0 { Duration::ZERO } else { Duration::from_secs_f64(-tokens / PACE_PER_SECOND) }
    }
}

impl Pacer {
    fn new() -> Self {
        Self { state: Mutex::new((PACE_BURST, Instant::now())) }
    }
}

fn pacer() -> &'static Pacer {
    static PACER: OnceLock<Pacer> = OnceLock::new();
    PACER.get_or_init(Pacer::new)
}

/// Whether calls to `url` are paced: all but this computer's own sequencer.
pub fn paced(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let host = match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    !matches!(host.to_ascii_lowercase().as_str(), "127.0.0.1" | "localhost" | "::1")
}

// ── The wallet's client ──

/// The `Retry-After` of the last 429, read off the HTTP response (jsonrpsee keeps only the
/// status). 0: none; otherwise the seconds plus one.
type RetryAfter = Arc<AtomicU64>;

fn retry_after_of(headers: &http::HeaderMap) -> Option<u64> {
    headers.get(http::header::RETRY_AFTER)?.to_str().ok()?.trim().parse().ok()
}

/// The HTTP layer that notes a 429's `Retry-After`.
#[derive(Clone)]
pub struct NoteRetryAfter<S> {
    inner: S,
    slot: RetryAfter,
}

#[derive(Clone)]
pub struct NoteRetryAfterLayer(RetryAfter);

impl<S> tower::Layer<S> for NoteRetryAfterLayer {
    type Service = NoteRetryAfter<S>;
    fn layer(&self, inner: S) -> Self::Service {
        NoteRetryAfter { inner, slot: self.0.clone() }
    }
}

impl<S, Request, Body> tower::Service<Request> for NoteRetryAfter<S>
where
    S: tower::Service<Request, Response = http::Response<Body>>,
    S::Future: Send + 'static,
    S::Error: 'static,
    Body: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let response = self.inner.call(request);
        let slot = self.slot.clone();
        Box::pin(async move {
            let response = response.await?;
            if response.status() == http::StatusCode::TOO_MANY_REQUESTS {
                slot.store(retry_after_of(response.headers()).map_or(0, |seconds| seconds.saturating_add(1)), Ordering::Relaxed);
            }
            Ok(response)
        })
    }
}

type Inner = HttpClient<RpcLogger<RpcService<NoteRetryAfter<HttpBackend>>>>;

/// Sees a proven transaction before it is sent; `Err` stops it.
pub type SendCheck = Box<dyn FnOnce(&LeeTransaction) -> Result<(), String> + Send>;

/// Whether sends are held for a check (`names proof prove`).
#[derive(Default)]
enum Hold {
    #[default]
    Off,
    Armed(SendCheck),
    /// The check was used: nothing more is sent by this process, a repriced second proof included.
    Spent,
}

impl Hold {
    /// Whether `tx` may go.
    fn admit(held: &Mutex<Hold>, tx: &LeeTransaction) -> Result<(), String> {
        match Self::take(held)? {
            Some(check) => check(tx),
            None => Ok(()),
        }
    }

    /// The check the next send must pass (none when sends aren't held). It is spent as it is
    /// taken, before it runs, so whatever it answers, nothing after it is sent.
    fn take(held: &Mutex<Hold>) -> Result<Option<SendCheck>, String> {
        let mut held = held.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match std::mem::replace(&mut *held, Hold::Spent) {
            Hold::Off => {
                *held = Hold::Off;
                Ok(None)
            }
            Hold::Armed(check) => Ok(Some(check)),
            Hold::Spent => Err("only one proven transaction may be sent".to_owned()),
        }
    }
}

/// The client `names` gives the wallet: jsonrpsee's, paced and tried again on a 429. Cheap to
/// clone; clones share the pacer and the `Retry-After` note.
#[derive(Clone)]
pub struct Rpc {
    inner: Inner,
    retry_after: RetryAfter,
    paced: bool,
    backoff: Backoff,
    hold: Arc<Mutex<Hold>>,
}

impl Rpc {
    /// From now on this client (and its clones) sends one transaction at most, and only if
    /// `check` lets it through.
    pub fn hold_send(&self, check: SendCheck) {
        *self.hold.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Hold::Armed(check);
    }

    /// A client for the sequencer at `url`, sending `Authorization: Basic <basic_auth>` when
    /// given, as the wallet's own client does (`WalletCore::client_from_config`).
    pub fn new(url: &str, basic_auth: Option<String>) -> Result<Self> {
        let retry_after = RetryAfter::default();
        let mut builder = HttpClientBuilder::default();
        if let Some(basic_auth) = basic_auth {
            let mut headers = http::HeaderMap::new();
            headers.insert(http::header::AUTHORIZATION, format!("Basic {basic_auth}").parse().context("Invalid basic auth format")?);
            builder = builder.set_headers(headers);
        }
        let inner = builder
            .set_http_middleware(tower::ServiceBuilder::new().layer(NoteRetryAfterLayer(retry_after.clone())))
            .build(url)
            .context("Failed to create sequencer client")?;
        Ok(Self { inner, retry_after, paced: paced(url), backoff: Backoff::DEFAULT, hold: Arc::default() })
    }

    /// The client for the wallet whose config is at `config_path`, with `overrides` applied:
    /// the sequencer and credentials the wallet would have used.
    pub fn for_wallet(config_path: &std::path::Path, overrides: Option<&wallet::config::WalletConfigOverrides>) -> Result<Self> {
        let mut config = wallet::config::WalletConfig::from_path_or_initialize_default(config_path)
            .with_context(|| format!("Failed to deserialize wallet config at {}", config_path.display()))?;
        if let Some(overrides) = overrides {
            config.apply_overrides(overrides.clone());
        }
        Self::new(config.sequencer.sequencer_addr.as_str(), config.sequencer.basic_auth.map(|auth| auth.to_string()))
    }

    #[cfg(test)]
    fn with_backoff(mut self, backoff: Backoff) -> Self {
        self.backoff = backoff;
        self
    }

    /// Runs `attempt` until it is not refused for going too fast, at most [`RETRIES`] times
    /// more, pacing each try.
    ///
    /// This tries again sends too (`sendTransaction`): a 429 or a -32005 is the RPC proxy (or
    /// Cloudflare in front of it) turning the request away at the door, before it reaches the
    /// sequencer, so the transaction was never accepted and sending it again cannot send it
    /// twice. Nothing else is tried again: a timeout or a dropped connection may come after
    /// the sequencer took the transaction.
    async fn retried<T, F, Fut>(&self, mut attempt: F) -> Result<T, ClientError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, ClientError>>,
    {
        let mut retry = 0;
        loop {
            if self.paced {
                let wait = pacer().reserve();
                if !wait.is_zero() {
                    tokio::time::sleep(wait).await;
                }
            }
            self.retry_after.store(0, Ordering::Relaxed);
            let error = match attempt().await {
                Err(error) if rate_limited(&error) => error,
                done => return done,
            };
            drop(error);
            if retry == RETRIES {
                return Err(ClientError::Transport(Box::new(Busy)));
            }
            let retry_after = self.retry_after.load(Ordering::Relaxed).checked_sub(1);
            tokio::time::sleep(self.backoff.wait(retry, retry_after)).await;
            retry += 1;
        }
    }
}

/// Whether the RPC refused `error`'s call for going too fast: HTTP 429 (jsonrpsee keeps only
/// the status) or JSON-RPC error -32005.
pub fn rate_limited(error: &ClientError) -> bool {
    match error {
        ClientError::Call(object) => object.code() == RATE_LIMITED_CODE,
        ClientError::Transport(cause) => {
            matches!(
                cause.downcast_ref::<jsonrpsee::http_client::transport::Error>(),
                Some(jsonrpsee::http_client::transport::Error::Rejected { status_code: 429 })
            ) || cause.to_string().contains("`429`")
        }
        _ => false,
    }
}

/// Parameters already serialized, so a call can be sent again.
struct Raw(Option<Box<RawValue>>);

impl ToRpcParams for Raw {
    fn to_rpc_params(self) -> Result<Option<Box<RawValue>>, serde_json::Error> {
        Ok(self.0)
    }
}

impl ClientT for Rpc {
    fn notification<Params>(&self, method: &str, params: Params) -> impl Future<Output = Result<(), ClientError>> + Send
    where
        Params: ToRpcParams + Send,
    {
        async move {
            let params = params.to_rpc_params()?;
            self.retried(|| self.inner.notification(method, Raw(params.clone()))).await
        }
    }

    fn request<R, Params>(&self, method: &str, params: Params) -> impl Future<Output = Result<R, ClientError>> + Send
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        async move {
            let params = params.to_rpc_params()?;
            self.retried(|| self.inner.request(method, Raw(params.clone()))).await
        }
    }

    fn batch_request<'a, R>(&self, batch: BatchRequestBuilder<'a>) -> impl Future<Output = Result<BatchResponse<'a, R>, ClientError>> + Send
    where
        R: DeserializeOwned + std::fmt::Debug + 'a,
    {
        async move { self.retried(|| self.inner.batch_request(batch.clone())).await }
    }
}

/// The wallet's calls, each through [`ClientT`] above (`RpcClient` is implemented for any
/// `ClientT`), so every one is paced and tried again.
impl wallet::WalletSequencerClient for Rpc {
    type Error = ClientError;

    async fn send_transaction(&self, tx: LeeTransaction) -> Result<HashType, Self::Error> {
        Hold::admit(&self.hold, &tx).map_err(|why| ClientError::Custom(format!("not sent: {why}")))?;
        RpcClient::send_transaction(self, tx).await
    }

    async fn get_block(&self, block_id: BlockId) -> Result<Option<Block>, Self::Error> {
        RpcClient::get_block(self, block_id).await
    }

    async fn get_block_range(&self, start_block_id: BlockId, end_block_id: BlockId) -> Result<Vec<Block>, Self::Error> {
        RpcClient::get_block_range(self, start_block_id, end_block_id).await
    }

    async fn get_last_block_id(&self) -> Result<BlockId, Self::Error> {
        RpcClient::get_last_block_id(self).await
    }

    async fn get_account_balance(&self, account_id: AccountId) -> Result<u128, Self::Error> {
        RpcClient::get_account_balance(self, account_id).await
    }

    async fn get_transaction(&self, tx_hash: HashType) -> Result<Option<(LeeTransaction, BlockId)>, Self::Error> {
        RpcClient::get_transaction(self, tx_hash).await
    }

    async fn get_accounts_nonces(&self, account_ids: Vec<AccountId>) -> Result<Vec<Nonce>, Self::Error> {
        RpcClient::get_accounts_nonces(self, account_ids).await
    }

    async fn get_account_view(&self, actor_state_selector: Actor) -> Result<Option<Account>, Self::Error> {
        RpcClient::get_account_view(self, actor_state_selector).await
    }

    async fn get_proofs_and_root(
        &self,
        commitments: Vec<Commitment>,
        message_positions: Vec<CommitmentPosition>,
    ) -> Result<(Vec<Option<MembershipProof>>, Vec<Option<MembershipProof>>, CommitmentSetDigest), Self::Error> {
        RpcClient::get_proofs_and_root(self, commitments, message_positions).await
    }

    async fn get_fee_state(&self) -> Result<sequencer_service_rpc::FeeStateQuote, Self::Error> {
        RpcClient::get_fee_state(self).await
    }

    async fn get_account(&self, account_id: AccountId) -> Result<Option<Account>, Self::Error> {
        RpcClient::get_account(self, account_id).await
    }

    async fn get_program_ids(&self) -> Result<std::collections::BTreeMap<String, ProgramId>, Self::Error> {
        RpcClient::get_program_ids(self).await
    }

    async fn get_publications(
        &self,
        from_position: CommitmentPosition,
        limit: PageLimit,
    ) -> Result<Vec<(CommitmentPosition, Publication)>, Self::Error> {
        RpcClient::get_publications(self, from_position, limit).await
    }

    async fn get_recovery_binding(&self, address: AccountId) -> Result<Option<EncryptedNote>, Self::Error> {
        RpcClient::get_recovery_binding(self, address).await
    }
}

// ── Plain calls (ureq) ──

/// POSTs the JSON-RPC `body` to `url` and returns the answer's text, paced and tried again like
/// [`Rpc`]'s calls. `send` makes one try: the status, the `Retry-After` seconds, the text.
pub fn post_with<F>(url: &str, backoff: Backoff, mut send: F) -> Result<String>
where
    F: FnMut() -> Result<(u16, Option<u64>, String)>,
{
    let mut retry = 0;
    loop {
        if paced(url) {
            std::thread::sleep(pacer().reserve());
        }
        let (status, retry_after, text) = send()?;
        let refused = status == 429
            || serde_json::from_str::<serde_json::Value>(&text).is_ok_and(|reply| reply["error"]["code"].as_i64() == Some(RATE_LIMITED_CODE.into()));
        if !refused {
            return Ok(text);
        }
        if retry == RETRIES {
            return Err(Busy.into());
        }
        std::thread::sleep(backoff.wait(retry, retry_after));
        retry += 1;
    }
}

/// POSTs the JSON-RPC `body` to the sequencer at `url` and returns the answer's text, however
/// the HTTP status reads (but a 429, tried again as above).
pub fn post(url: &str, body: &serde_json::Value) -> Result<String> {
    post_with(url, Backoff::DEFAULT, || {
        let mut response = ureq::post(url)
            .config()
            .http_status_as_error(false)
            .build()
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .with_context(|| format!("cannot reach the sequencer at {url}"))?;
        let status = response.status().as_u16();
        let retry_after = retry_after_of(response.headers());
        Ok((status, retry_after, response.body_mut().read_to_string().unwrap_or_default()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};
    use anyhow::anyhow;
    use std::sync::atomic::AtomicUsize;

    const QUICK: Backoff = Backoff { first: Duration::from_millis(1), cap: Duration::from_millis(5) };

    /// A fake sequencer on 127.0.0.1: answers each request with the next of `answers`
    /// (status, extra headers, body), repeating the last; counts the requests.
    fn fake_rpc(answers: Vec<(u16, &'static str, &'static str)>) -> (String, Arc<AtomicUsize>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let counted = count.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                // One request per connection: read the head and the body it announces.
                let mut seen = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    seen.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&seen);
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text[..end]
                            .lines()
                            .find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                            .unwrap_or(0);
                        if seen.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let at = counted.fetch_add(1, Ordering::SeqCst);
                let (status, headers, body) = answers[at.min(answers.len() - 1)];
                // Answer to the request's own id, as a sequencer does.
                let text = String::from_utf8_lossy(&seen);
                let request = text.split_once("\r\n\r\n").map_or("", |(_, body)| body);
                let id = serde_json::from_str::<serde_json::Value>(request).map_or(serde_json::Value::Null, |request| request["id"].clone());
                let body = body.replace("\"id\":0", &format!("\"id\":{id}"));
                let reason = if status == 200 { "OK" } else { "Too Many Requests" };
                let reply = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        (url, count)
    }

    const LIMITED: (u16, &str, &str) = (429, "Retry-After: 10\r\n", r#"{"jsonrpc":"2.0","id":0,"error":{"code":-32005,"message":"rate limited; slow down"}}"#);
    const BLOCK_7: (u16, &str, &str) = (200, "", r#"{"jsonrpc":"2.0","id":0,"result":7}"#);

    #[tokio::test]
    async fn a_429_twice_then_an_answer_succeeds() {
        let (url, count) = fake_rpc(vec![LIMITED, LIMITED, BLOCK_7]);
        let rpc = Rpc::new(&url, None).unwrap().with_backoff(QUICK);
        let last = wallet::WalletSequencerClient::get_last_block_id(&rpc).await.unwrap();
        assert_eq!((last, count.load(Ordering::SeqCst)), (7, 3));
    }

    #[tokio::test]
    async fn always_429_fails_plainly_after_the_cap() {
        let (url, count) = fake_rpc(vec![LIMITED]);
        let rpc = Rpc::new(&url, None).unwrap().with_backoff(QUICK);
        let error = wallet::WalletSequencerClient::get_last_block_id(&rpc).await.unwrap_err();
        assert_eq!(error.to_string(), BUSY);
        assert_eq!(count.load(Ordering::SeqCst), 1 + RETRIES as usize);
        // The last 429's Retry-After (10) was read off the HTTP answer; the waits held it to
        // QUICK's cap, or this test would take 40 s.
        assert_eq!(rpc.retry_after.load(Ordering::Relaxed), 10 + 1);
        // As the wallet and then `names` show it: the plain line on top, the step below.
        let shown = plain_when_busy(anyhow::Error::from(error).context("cannot sync the private accounts"));
        assert_eq!(shown.to_string(), BUSY);
        let full = format!("{shown:?}");
        assert!(full.starts_with(&format!("{BUSY}\n\nCaused by:")) && full.contains("cannot sync the private accounts"), "{full}");
    }

    #[test]
    fn a_message_that_already_says_busy_is_kept() {
        let said = format!("1 of 2 received. The claim stopped: {BUSY}. It and the rest still wait, nothing of them is lost: run `names receive` again");
        assert_eq!(plain_when_busy(anyhow!(said.clone())).to_string(), said);
        assert_eq!(plain_when_busy(anyhow!("cannot reach the sequencer")).to_string(), "cannot reach the sequencer");
    }

    #[test]
    fn the_pacer_lets_a_burst_of_8_through_then_4_a_second() {
        let pacer = Pacer::new();
        for _ in 0..8 {
            assert_eq!(pacer.reserve(), Duration::ZERO);
        }
        let (ninth, tenth) = (pacer.reserve(), pacer.reserve());
        assert!(ninth > Duration::from_millis(240) && ninth <= Duration::from_millis(250), "{ninth:?}");
        assert!(tenth > Duration::from_millis(490) && tenth <= Duration::from_millis(500), "{tenth:?}");
    }

    #[tokio::test]
    async fn a_minus_32005_with_200_is_tried_again_and_other_errors_are_not() {
        let (url, count) = fake_rpc(vec![(200, "", r#"{"jsonrpc":"2.0","id":0,"error":{"code":-32005,"message":"rate limited; slow down"}}"#), BLOCK_7]);
        let rpc = Rpc::new(&url, None).unwrap().with_backoff(QUICK);
        assert_eq!(wallet::WalletSequencerClient::get_last_block_id(&rpc).await.unwrap(), 7);
        assert_eq!(count.load(Ordering::SeqCst), 2);

        let (url, count) = fake_rpc(vec![(500, "", "boom"), BLOCK_7]);
        let rpc = Rpc::new(&url, None).unwrap().with_backoff(QUICK);
        assert!(wallet::WalletSequencerClient::get_last_block_id(&rpc).await.is_err());
        assert_eq!(count.load(Ordering::SeqCst), 1, "a 500 is not tried again");
    }

    #[test]
    fn the_retry_after_is_noted() {
        let mut headers = http::HeaderMap::new();
        headers.insert(http::header::RETRY_AFTER, "10".parse().unwrap());
        assert_eq!(retry_after_of(&headers), Some(10));
        assert_eq!(retry_after_of(&http::HeaderMap::new()), None);
    }

    #[test]
    fn the_waits() {
        let default = Backoff::DEFAULT;
        let waits: Vec<u64> = (0..RETRIES).map(|retry| default.wait(retry, None).as_secs()).collect();
        assert_eq!(waits, [2, 4, 8, 10]);
        assert_eq!(default.wait(0, Some(3)), Duration::from_secs(3));
        assert_eq!(default.wait(0, Some(60)), Duration::from_secs(10), "Retry-After is held to 10 s");
    }

    #[test]
    fn plain_calls_are_tried_again_too() {
        let (url, count) = fake_rpc(vec![LIMITED, LIMITED, BLOCK_7]);
        let mut tries = 0;
        let text = post_with(&url, QUICK, || {
            tries += 1;
            let mut response = ureq::post(&url).config().http_status_as_error(false).build().send("{}")?;
            Ok((response.status().as_u16(), retry_after_of(response.headers()), response.body_mut().read_to_string()?))
        })
        .unwrap();
        assert!(text.contains("\"result\":7"));
        assert_eq!((tries, count.load(Ordering::SeqCst)), (3, 3));

        let (url, count) = fake_rpc(vec![LIMITED]);
        let error = post_with(&url, QUICK, || {
            let mut response = ureq::post(&url).config().http_status_as_error(false).build().send("{}")?;
            Ok((response.status().as_u16(), retry_after_of(response.headers()), response.body_mut().read_to_string()?))
        })
        .unwrap_err();
        assert_eq!(format!("{error:#}"), BUSY);
        assert_eq!(count.load(Ordering::SeqCst), 1 + RETRIES as usize);
    }

    #[test]
    fn only_remote_sequencers_are_paced() {
        assert!(!paced("http://127.0.0.1:3040"));
        assert!(!paced("http://localhost:3040/"));
        assert!(!paced("http://[::1]:3040"));
        assert!(paced("https://lez.nameswallet.app/"));
        assert!(paced("https://127.0.0.1.example.com/"));
    }
}

#[cfg(test)]
mod hold_tests {
    use super::*;

    #[test]
    fn a_held_client_lets_one_checked_send_out_and_never_a_second() {
        let rpc = Rpc::new("http://127.0.0.1:9", None).unwrap();
        for _ in 0..2 {
            assert!(Hold::take(&rpc.hold).unwrap().is_none(), "sends aren't held until asked");
        }
        let clone = rpc.clone();
        rpc.hold_send(Box::new(|_| Err("no".to_owned())));
        assert!(Hold::take(&clone.hold).unwrap().is_some(), "a clone made earlier is held too");
        for client in [&rpc, &clone, &rpc] {
            assert!(Hold::take(&client.hold).is_err(), "nothing more may be sent");
        }
    }
}
