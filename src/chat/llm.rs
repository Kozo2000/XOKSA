//! LLM provider switching, model resolution, and chat-turn dispatch (`/llm`).
//! Operates on `ChatSession`/`Config` via descendant-module access.

use super::*;

#[derive(Debug, PartialEq)]
pub(super) enum LlmSwitchCmd<'a> {
    /// `/llm` — list usable LLMs (provider/model/key + ollama aliases).
    List,
    /// `/llm net` — query each provider's API for current model names.
    Net,
    Switch {
        provider: &'a str,
        explicit_model: &'a str,
    },
    UnknownProvider(&'a str),
}

pub(super) fn parse_llm_switch_command(input: &str) -> Option<LlmSwitchCmd<'_>> {
    if input != "/llm" && !input.starts_with("/llm ") {
        return None;
    }
    let arg = input.strip_prefix("/llm ").unwrap_or("").trim();
    if arg.is_empty() {
        return Some(LlmSwitchCmd::List);
    }
    if arg == "net" {
        return Some(LlmSwitchCmd::Net);
    }
    let (provider, explicit_model) = match arg.split_once(':') {
        Some((p, m)) => (p.trim(), m.trim()),
        None => (arg, ""),
    };
    const VALID: &[&str] = &["openai", "gemini", "claude", "ollama"];
    if VALID.contains(&provider) {
        Some(LlmSwitchCmd::Switch {
            provider,
            explicit_model,
        })
    } else {
        Some(LlmSwitchCmd::UnknownProvider(provider))
    }
}

pub(super) fn resolve_llm_default_model(provider: &str, config: &Config) -> String {
    match provider {
        "openai" => config.openai_model.clone(),
        "gemini" => config.gemini_model.clone(),
        "claude" => config.claude_model.clone(),
        _ => String::new(),
    }
}

pub(crate) fn active_llm_model(config: &Config) -> String {
    if config.llm_model.is_empty() {
        resolve_llm_default_model(&config.llm_provider, config)
    } else {
        config.llm_model.clone()
    }
}

/// The `provider/model` badge for this config's effective LLM — the SINGLE source of
/// the badge string (used as the header/fallback label wherever no persisted chat
/// session overrides it), so the format never drifts between endpoints.
pub(crate) fn env_llm_label(config: &Config) -> String {
    format!(
        "{}/{}",
        provider_display_name(&config.llm_provider),
        active_llm_model(config)
    )
}

/// Human display name for a provider id (badge display only; the raw lowercase id
/// is still what `config.llm_provider` stores and what dispatch/persistence use).
fn provider_display_name(provider: &str) -> &str {
    match provider.trim() {
        "openai" => "OpenAI",
        "gemini" => "Gemini",
        "claude" => "Claude",
        "ollama" => "Ollama",
        other => other,
    }
}

/// The "🧠 <provider>/<model> が解説:" attribution line shown above every LLM
/// answer. Single source so CLI, Web chat, `/basic`, and multi-timeframe read
/// identically — the numbers are the program's; this names who wrote the prose.
pub(crate) fn llm_commentary_badge(config: &Config, lang: &str) -> String {
    llm_commentary_badge_stated(config, lang, "", "")
}

/// Attribution badge for a `/crit` critique. A critique is an OPINION (non-SOT), so
/// it names its author — the active LLM — mirroring `llm_commentary_badge` and using
/// the same `env_llm_label` so the model shown never drifts between the two.
pub(crate) fn llm_critique_badge(config: &Config, lang: &str) -> String {
    let label = env_llm_label(config);
    if lang == "ja" {
        format!("🧪 {} が批評:", label)
    } else {
        format!("🧪 {} critique:", label)
    }
}

/// Same badge with an optional parenthesised state note (e.g. the Ollama guard
/// state for one-shot analysis: `guarded: integrity check` / `unguarded`).
/// `state_ja` / `state_en` are the already-localised note bodies; pass `""` for
/// the plain badge. Kept as one function so the badge format never drifts.
pub(crate) fn llm_commentary_badge_stated(
    config: &Config,
    lang: &str,
    state_ja: &str,
    state_en: &str,
) -> String {
    let label = env_llm_label(config);
    let ticker = config.ticker.trim();
    if lang == "ja" {
        let note = if state_ja.is_empty() {
            String::new()
        } else {
            format!("（{}）", state_ja)
        };
        if ticker.is_empty() {
            format!("🧠 {} が解説{}:", label, note)
        } else {
            format!("🧠 {} が {} を解説{}:", label, ticker, note)
        }
    } else {
        let note = if state_en.is_empty() {
            String::new()
        } else {
            format!(" ({})", state_en)
        };
        if ticker.is_empty() {
            format!("🧠 commentary by {}{}:", label, note)
        } else {
            format!("🧠 commentary by {} on {}{}:", label, ticker, note)
        }
    }
}

/// `/llm` — list usable LLMs locally (no network): each cloud provider's
/// effective model (env/default) and whether its API key is present, the current
/// selection (`*`), plus any configured ollama instances. The model comes from
/// config/default — the list never invents model names (SOT).
pub(super) fn render_llm_list(config: &Config, out: &mut ChatOut, lang: &str) {
    out.line(if lang == "ja" {
        "使用可能なLLM:"
    } else {
        "Available LLMs:"
    });
    for (p, key, model) in config.cloud_provider_rows() {
        let has_key = crate::utils::resolve_api_key(key).ok().flatten().is_some();
        let current = config.llm_provider == p && config.ollama_alias.is_empty();
        let marker = if current { "*" } else { " " };
        let status = match (lang, has_key) {
            ("ja", true) => "key:あり",
            ("ja", false) => "key:なし（使用不可）",
            (_, true) => "key:yes",
            (_, false) => "key:no (unusable)",
        };
        out.line(format!("{} {:<7} {:<26} [{}]", marker, p, model, status));
    }
    for inst in live_ollama_instances() {
        let current = config.llm_provider == "ollama" && config.ollama_alias == inst.alias;
        let marker = if current { "*" } else { " " };
        out.line(format!(
            "{} ollama:{:<8} {:<16} [{}:{}]",
            marker, inst.alias, inst.model, inst.host, inst.port
        ));
    }
    out.line(if lang == "ja" {
        "  * = 現在 / 切替: llm <provider> / 現行モデル取得: llm net"
    } else {
        "  * = current / switch: llm <provider> / discover: llm net"
    });
}

async fn http_get_json(
    config: &Config,
    url: &str,
    // Class A: header values are borrowed `&str`, never an owned `String`, so an API
    // key is not copied into a non-zeroized buffer on the way in (security-design
    // §0.1). The only remaining copy is reqwest's internal HeaderValue — the
    // documented transmission-boundary exception.
    headers: &[(&str, &str)],
) -> anyhow::Result<serde_json::Value> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.llm_timeout_secs))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(p) = config.https_proxy.as_deref() {
        builder = builder.proxy(crate::utils::build_proxy(p, config.no_proxy.as_deref())?);
    }
    let client = builder.build()?;
    let mut req = client.get(url);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    // `without_url()` strips the request URL from the error. Critical: some
    // providers (Gemini) historically carried the API key as a `?key=` query
    // param, and reqwest's error Display includes the URL — surfacing that error
    // would leak the key into chat output/logs (security-design §0.1 Class A:
    // keys must never appear in logs or error messages).
    let resp = req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e.without_url()))?;
    let status = resp.status();
    let body = resp.text().await?;
    anyhow::ensure!(
        status.is_success(),
        "HTTP {} — {}",
        status.as_u16(),
        body.chars().take(160).collect::<String>()
    );
    Ok(serde_json::from_str(&body)?)
}

fn emit_model_ids(
    out: &mut ChatOut,
    provider: &str,
    arr: Option<&Vec<serde_json::Value>>,
    field: &str,
) {
    out.line(format!("{}:", provider));
    let Some(items) = arr else {
        out.line("  (no models returned)");
        return;
    };
    for item in items {
        if let Some(id) = item[field].as_str() {
            // Gemini returns "models/gemini-…"; show the bare name.
            let id = id.strip_prefix("models/").unwrap_or(id);
            out.line(format!("  {}", id));
        }
    }
}

/// `/llm net` — query each provider that has an API key for its current model
/// names (a discovery aid for filling `*_model` in xoksa.env, since providers
/// rename models over time). Best-effort: a provider that errors is reported and
/// skipped.
pub(super) async fn query_models_net(config: &Config, out: &mut ChatOut, lang: &str) {
    out.line(if lang == "ja" {
        "各社APIから現行モデルを取得中..."
    } else {
        "Querying provider model lists..."
    });
    let mut any = false;
    if let Some(key) = crate::utils::resolve_api_key("OPENAI_API_KEY")
        .ok()
        .flatten()
    {
        any = true;
        // Zeroizing so the "Bearer <key>" concatenation (which contains the key) is
        // wiped on drop; only a borrowed `&str` crosses into http_get_json.
        let auth = zeroize::Zeroizing::new(format!("Bearer {}", key.as_str()));
        match http_get_json(
            config,
            "https://api.openai.com/v1/models",
            &[("Authorization", auth.as_str())],
        )
        .await
        {
            Ok(v) => emit_model_ids(out, "openai", v["data"].as_array(), "id"),
            Err(e) => out.err(format!("openai: {e}")),
        }
    }
    if let Some(key) = crate::utils::resolve_api_key("GEMINI_API_KEY")
        .ok()
        .flatten()
    {
        any = true;
        // Key goes in the `x-goog-api-key` header, never the URL — matching the
        // production Gemini call (llm.rs) and keeping the key out of URLs/logs.
        match http_get_json(
            config,
            "https://generativelanguage.googleapis.com/v1beta/models",
            &[("x-goog-api-key", key.as_str())],
        )
        .await
        {
            Ok(v) => emit_model_ids(out, "gemini", v["models"].as_array(), "name"),
            Err(e) => out.err(format!("gemini: {e}")),
        }
    }
    if let Some(key) = crate::utils::resolve_api_key("CLAUDE_API_KEY")
        .ok()
        .flatten()
    {
        any = true;
        match http_get_json(
            config,
            "https://api.anthropic.com/v1/models",
            &[
                ("x-api-key", key.as_str()),
                ("anthropic-version", "2023-06-01"),
            ],
        )
        .await
        {
            Ok(v) => emit_model_ids(out, "claude", v["data"].as_array(), "id"),
            Err(e) => out.err(format!("claude: {e}")),
        }
    }
    if !any {
        out.line(if lang == "ja" {
            "（APIキーが設定されたクラウドLLMがありません）"
        } else {
            "(No cloud LLM has an API key configured)"
        });
    }
}

pub(super) async fn send_chat_input_to_llm(
    session: &mut ChatSession,
    chat_config: &Config,
    cumulative_tokens: &mut ChatTokenUsage,
    user_input: String,
    lang: &str,
    interactive: bool,
    out: &mut ChatOut,
) {
    // The confirmed data of the loaded instruments; the output guard verifies
    // the model's numbers against these structures, not against the prompt text.
    let facts = session.confirmed_facts();
    let prompt = session.build_prompt(&user_input);
    let saved_pending = session.news_extra_pending_text.take();
    let Some(result) = super::exec::await_or_cancel(
        crate::llm::send_chat_turn_with_usage(chat_config, &prompt, Some(&facts)),
        interactive,
        lang,
        out,
    )
    .await
    else {
        // Cancelled: restore the pending extra-news injection for the next turn.
        session.news_extra_pending_text = saved_pending;
        return;
    };
    match result {
        Ok((response, usage)) => {
            out.line(llm_commentary_badge(chat_config, lang));
            out.line(format!("\n{}\n", response));
            cumulative_tokens.accumulate(usage);
            let model = active_llm_model(chat_config);
            session.add_debate_entry(
                &chat_config.llm_provider,
                &model,
                &chat_config.ollama_alias,
                &user_input,
                &response,
            );
            session.add_turn(user_input, response);
        }
        Err(e) => out.err(format!("❌ {}", e)),
    }
}

pub(crate) fn live_ollama_instances() -> Vec<crate::config::OllamaInstance> {
    // Reuse the single env-file loader (SOT §4.2). `scan_ollama_instances` reads only
    // `OLLAMA_*` keys, which `load_env_map` keeps (it filters only Class A), so the
    // result is unchanged while the duplicate parse loop is removed.
    crate::config::scan_ollama_instances(&crate::bootstrap::load_env_map())
}
