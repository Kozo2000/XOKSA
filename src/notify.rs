//! Chat notification (push): send a one-line message to a chat platform
//! (Slack / Discord / Google Chat via an incoming webhook, LINE via the
//! Messaging API).
//!
//! Outbound is restricted to a **fixed per-platform host allowlist** — the user
//! supplies only the secret (the webhook URL, or the LINE bot token), and any
//! webhook URL whose host is not on the allowlist is rejected, so the §4
//! anti-SSRF property (outbound hosts fixed in code, never taken from input) is
//! preserved. Data-POST only: no shell, no arbitrary execution. The message
//! carries a computed (SOT) value, never a fabricated trade call. See
//! docs/dev-prog/security-design.md §4.
#![allow(async_fn_in_trait)]

use anyhow::{bail, Context, Result};
use zeroize::Zeroizing;

/// DI seam for a chat notifier — kept a trait so the monitor (Phase 4) can be
/// tested against a mock. Concrete platforms dispatch by `NotifierKind` inside
/// `HttpNotifier` (enum dispatch, not `dyn`; same rationale as the `PriceFetcher`
/// seam — async-fn-in-trait is awkward with trait objects, and the platform set
/// is bounded).
pub trait Notifier {
    async fn send(
        &self,
        message: &str,
        https_proxy: Option<&str>,
        no_proxy: Option<&str>,
    ) -> Result<()>;
}

/// A supported chat platform (`NOTIFY_<n>_KIND`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifierKind {
    Slack,
    Discord,
    GoogleChat,
    Line,
}

impl NotifierKind {
    /// Parse a `NOTIFY_<n>_KIND` value; unknown platforms are rejected.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "slack" => Some(Self::Slack),
            "discord" => Some(Self::Discord),
            "gchat" => Some(Self::GoogleChat),
            "line" => Some(Self::Line),
            _ => None,
        }
    }

    /// The canonical `NOTIFY_<n>_KIND` token for this platform (inverse of `parse`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Slack => "slack",
            Self::Discord => "discord",
            Self::GoogleChat => "gchat",
            Self::Line => "line",
        }
    }

    /// The outbound hosts allowed for this platform (the SSRF host allowlist).
    fn allowed_hosts(self) -> &'static [&'static str] {
        match self {
            Self::Slack => &["hooks.slack.com"],
            Self::Discord => &["discord.com", "discordapp.com"],
            Self::GoogleChat => &["chat.googleapis.com"],
            Self::Line => &["api.line.me"],
        }
    }

    /// JSON body for the webhook platforms (LINE is built separately in `send`).
    fn webhook_payload(self, message: &str) -> serde_json::Value {
        match self {
            // Slack and Google Chat both take `{ "text": … }`.
            Self::Slack | Self::GoogleChat => serde_json::json!({ "text": message }),
            Self::Discord => serde_json::json!({ "content": message }),
            Self::Line => serde_json::json!({}),
        }
    }
}

/// A validated notifier bound to one channel.
pub struct HttpNotifier {
    kind: NotifierKind,
    /// Webhook kinds: the full validated `https` webhook URL. LINE: the bot
    /// (channel access) token. Class A — kept in `Zeroizing`.
    secret: Zeroizing<String>,
    /// LINE only: the destination id (`NOTIFY_<n>_TO`).
    to: Option<String>,
}

/// Build a notifier from a channel's kind, secret, and (LINE) destination id.
/// Validates up front: a webhook URL must be `https` with a host on the
/// platform allowlist; LINE requires a destination id.
pub fn build_notifier(
    kind: NotifierKind,
    secret: Zeroizing<String>,
    to: Option<String>,
) -> Result<HttpNotifier> {
    match kind {
        NotifierKind::Line => {
            let to = to
                .filter(|s| !s.trim().is_empty())
                .context("LINE notifier requires a destination id (NOTIFY_<n>_TO)")?;
            Ok(HttpNotifier {
                kind,
                secret,
                to: Some(to),
            })
        }
        _ => {
            validate_webhook_url(kind, &secret)?;
            Ok(HttpNotifier {
                kind,
                secret,
                to: None,
            })
        }
    }
}

/// Reject anything that is not an `https` URL whose host is on the platform's
/// allowlist — this is what keeps the outbound host fixed despite the secret
/// being user-supplied.
fn validate_webhook_url(kind: NotifierKind, url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url.trim()).context("invalid webhook URL")?;
    if parsed.scheme() != "https" {
        bail!("webhook URL must be https");
    }
    let host = parsed.host_str().unwrap_or_default();
    if !kind.allowed_hosts().contains(&host) {
        bail!("webhook host is not on the allowlist for this platform");
    }
    Ok(())
}

impl HttpNotifier {
    /// Describe a transport failure **without the URL**.
    ///
    /// `reqwest`'s own `Display` appends ` for url (…)` (`error.rs:300`), and for
    /// Slack / Discord / Google Chat that URL *is* the secret — the user supplies
    /// the whole webhook. Measured: a connect failure printed
    /// `error sending request for url (https://hooks.slack.com/services/T…/B…/<token>)`,
    /// and that string was the one the monitor wrote to the diagnostic log and
    /// `/alert test` returned to the browser. Both are forbidden (§0.1: a Class A
    /// secret is never logged; §4: no credential crosses the browser boundary).
    ///
    /// So the message is rebuilt from the error's own classification plus its
    /// source chain, neither of which carries the URL, and the secret is then
    /// scrubbed from what is left — a second line of defense that also covers the
    /// LINE token and any platform error body that echoes what was sent.
    fn describe_failure(&self, e: reqwest::Error) -> anyhow::Error {
        let what = if e.is_connect() {
            "connection failed"
        } else if e.is_timeout() {
            "timed out"
        } else if e.is_decode() {
            "could not decode the response"
        } else if e.is_body() {
            "body error"
        } else {
            "request failed"
        };
        let mut detail = String::new();
        let mut source = std::error::Error::source(&e);
        while let Some(s) = source {
            if !detail.is_empty() {
                detail.push_str(": ");
            }
            detail.push_str(&s.to_string());
            source = s.source();
        }
        let body = if detail.is_empty() {
            what.to_string()
        } else {
            format!("{what}: {detail}")
        };
        anyhow::anyhow!("notification failed: {}", self.redact(&body))
    }

    /// Remove the secret from text that is about to be logged or displayed. The
    /// secret is a whole URL (webhook platforms) or a bearer token (LINE).
    fn redact(&self, text: &str) -> String {
        text.replace(self.secret.as_str(), "<redacted>")
    }
}

impl Notifier for HttpNotifier {
    async fn send(
        &self,
        message: &str,
        https_proxy: Option<&str>,
        no_proxy: Option<&str>,
    ) -> Result<()> {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none());
        if let Some(url) = https_proxy {
            builder = builder.proxy(crate::utils::build_proxy(url, no_proxy)?);
        }
        let client = builder.build()?;

        let resp = match self.kind {
            NotifierKind::Line => {
                let body = serde_json::json!({
                    "to": self.to.as_deref().unwrap_or_default(),
                    "messages": [{ "type": "text", "text": message }],
                });
                // Class A: pass the token as a borrowed `&str`; `bearer_auth` builds
                // the "Bearer <token>" header value inside reqwest, so the only copy
                // of the secret is reqwest's internal HeaderValue — the accepted
                // transmission-boundary exception (security-design §0.1), the same
                // one the J-Quants x-api-key header relies on. No owned `String`
                // holding the secret is created in our code.
                client
                    .post("https://api.line.me/v2/bot/message/push")
                    .bearer_auth(self.secret.as_str())
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| self.describe_failure(e))?
            }
            _ => {
                let body = self.kind.webhook_payload(message);
                client
                    .post(self.secret.as_str())
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| self.describe_failure(e))?
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            // Cap the length, and redact: the platforms do not echo the secret, but
            // this text reaches a log and the browser, so it does not rely on that.
            let detail: String = resp
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(300)
                .collect();
            bail!(
                "notification failed: HTTP {status}: {}",
                self.redact(&detail)
            );
        }
        Ok(())
    }
}

/// Non-interactive `xoksa test-notify`: read a channel `{kind,to,secret,n}` JSON on
/// stdin, send one test message through the same validated notifier the monitor
/// uses (the host allowlist is enforced here), and print `{"ok":true}`. The secret
/// arrives over stdin — never argv — and the buffer is zeroized after parsing. When
/// `secret` is empty and `n` is a saved channel number, the stored
/// `NOTIFY_<n>_SECRET` is read from the keychain (so an already-saved channel can be
/// tested without re-typing its secret).
pub async fn run_test_notify_cli() -> Result<()> {
    use std::io::Read;
    use zeroize::Zeroize;

    #[derive(serde::Deserialize)]
    struct TestInput {
        kind: String,
        #[serde(default)]
        to: String,
        #[serde(default)]
        secret: String,
        /// The saved channel number (`NOTIFY_<n>`), 0 for an unsaved channel.
        #[serde(default)]
        n: u8,
    }

    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    let parsed = serde_json::from_str::<TestInput>(&buf);
    buf.zeroize();
    let input = parsed.context("invalid test-notify JSON on stdin")?;

    let kind = NotifierKind::parse(&input.kind)
        .with_context(|| format!("unsupported notification platform: {}", input.kind))?;
    let to = Some(input.to).filter(|s| !s.trim().is_empty());

    // The typed secret takes priority; otherwise fall back to the saved one.
    let secret = if input.secret.trim().is_empty() {
        if input.n == 0 {
            bail!("no secret entered and no saved secret for this channel");
        }
        crate::keystore::get_key(&format!("NOTIFY_{}_SECRET", input.n))?.with_context(|| {
            format!(
                "no saved secret (NOTIFY_{}_SECRET) — enter one to test",
                input.n
            )
        })?
    } else {
        Zeroizing::new(input.secret)
    };
    let notifier = build_notifier(kind, secret, to)?;

    // Honor the configured proxy (xoksa.env), like every other outbound call.
    let env_map = crate::bootstrap::load_env_map();
    let https_proxy = env_map.get("HTTPS_PROXY").cloned();
    let no_proxy = env_map.get("NO_PROXY").cloned();

    notifier
        .send(
            "🔔 XOKSA test notification: this channel is configured correctly.",
            https_proxy.as_deref(),
            no_proxy.as_deref(),
        )
        .await?;
    println!("{{\"ok\":true}}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A send failure must not carry the webhook URL, because for Slack / Discord /
    /// Google Chat that URL *is* the secret. Measured before the fix: `reqwest`'s
    /// `Display` ends with ` for url (…)`, and that text was written to the
    /// diagnostic log by the monitor and returned to the browser by `/alert test`
    /// (§0.1, §4). The failure still has to be diagnosable, so the cause is
    /// asserted as well — a message that says nothing would "pass" this test.
    #[tokio::test]
    async fn a_send_failure_names_the_cause_without_the_webhook_secret() {
        let secret = Zeroizing::new(
            "https://hooks.slack.com/services/T00000000/B00000000/LEAKCANARY123".to_string(),
        );
        let n = build_notifier(NotifierKind::Slack, secret, None).expect("valid webhook");
        // An unreachable proxy makes the send fail at the transport layer.
        let err = n
            .send("probe", Some("http://127.0.0.1:1"), None)
            .await
            .expect_err("the send must fail");
        let text = format!("{err:#}");
        eprintln!("ERROR TEXT: {text}");
        assert!(
            !text.contains("LEAKCANARY123"),
            "the webhook secret appears in the error text: {text}"
        );
        assert!(
            !text.contains("hooks.slack.com"),
            "the webhook URL appears in the error text: {text}"
        );
        assert!(
            text.contains("connection failed"),
            "the error must still name the cause: {text}"
        );
    }

    /// Redaction is the second line of defense: whatever text is about to be
    /// logged or displayed, an occurrence of the secret is removed. This covers
    /// the LINE bearer token and a platform error body that echoes what was sent.
    #[test]
    fn redaction_removes_the_secret_from_text_that_will_be_shown() {
        let secret = Zeroizing::new(
            "https://hooks.slack.com/services/T00000000/B00000000/LEAKCANARY123".to_string(),
        );
        let n = build_notifier(NotifierKind::Slack, secret, None).expect("valid webhook");
        let shown = n.redact(
            "no_such_target: https://hooks.slack.com/services/T00000000/B00000000/LEAKCANARY123",
        );
        assert_eq!(shown, "no_such_target: <redacted>");
    }

    #[test]
    fn parse_known_and_unknown_kinds() {
        assert_eq!(NotifierKind::parse("slack"), Some(NotifierKind::Slack));
        assert_eq!(
            NotifierKind::parse(" Discord "),
            Some(NotifierKind::Discord)
        );
        assert_eq!(NotifierKind::parse("gchat"), Some(NotifierKind::GoogleChat));
        assert_eq!(NotifierKind::parse("LINE"), Some(NotifierKind::Line));
        assert_eq!(NotifierKind::parse("telegram"), None);
        assert_eq!(NotifierKind::parse(""), None);
    }

    #[test]
    fn webhook_host_must_be_on_allowlist() {
        assert!(validate_webhook_url(
            NotifierKind::Slack,
            "https://hooks.slack.com/services/T/B/xxx"
        )
        .is_ok());
        assert!(validate_webhook_url(
            NotifierKind::Discord,
            "https://discord.com/api/webhooks/1/abc"
        )
        .is_ok());
        // Wrong host for the platform.
        assert!(validate_webhook_url(NotifierKind::Slack, "https://evil.example/x").is_err());
        // A Discord URL is not valid for Slack.
        assert!(
            validate_webhook_url(NotifierKind::Slack, "https://discord.com/api/webhooks/1/a")
                .is_err()
        );
        // Must be https.
        assert!(validate_webhook_url(NotifierKind::Slack, "http://hooks.slack.com/x").is_err());
        // Not a URL.
        assert!(validate_webhook_url(NotifierKind::Slack, "not a url").is_err());
    }

    #[test]
    fn build_webhook_validates_and_line_requires_to() {
        // Webhook kind: bad host rejected, good host accepted.
        assert!(build_notifier(
            NotifierKind::Slack,
            Zeroizing::new("https://evil.example/x".to_string()),
            None
        )
        .is_err());
        assert!(build_notifier(
            NotifierKind::Slack,
            Zeroizing::new("https://hooks.slack.com/services/T/B/x".to_string()),
            None
        )
        .is_ok());
        // LINE requires a destination id.
        assert!(build_notifier(
            NotifierKind::Line,
            Zeroizing::new("token".to_string()),
            None
        )
        .is_err());
        assert!(build_notifier(
            NotifierKind::Line,
            Zeroizing::new("token".to_string()),
            Some("U123".to_string())
        )
        .is_ok());
    }

    #[test]
    fn webhook_payload_shapes() {
        assert_eq!(
            NotifierKind::Slack.webhook_payload("hi"),
            serde_json::json!({ "text": "hi" })
        );
        assert_eq!(
            NotifierKind::GoogleChat.webhook_payload("hi"),
            serde_json::json!({ "text": "hi" })
        );
        assert_eq!(
            NotifierKind::Discord.webhook_payload("hi"),
            serde_json::json!({ "content": "hi" })
        );
    }
}
