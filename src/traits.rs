//! Abstraction traits for HTTP dependencies (for dependency injection in tests)
#![allow(async_fn_in_trait)]

use crate::config::{AnalysisMode, Config};
use crate::market::MarketDataSnapshot;
use crate::news::Article;
use anyhow::Result;

pub trait PriceFetcher {
    async fn fetch_snapshot(&self, ticker: &str, mode: AnalysisMode) -> Result<MarketDataSnapshot>;
}

pub trait ArticleFetcher {
    #[allow(clippy::too_many_arguments)]
    async fn fetch_articles(
        &self,
        query: &str,
        api_key: &str,
        country: &str,
        search_lang: &str,
        ui_lang: &str,
        count: usize,
        freshness: Option<&str>,
    ) -> Result<Vec<Article>>;
}

pub trait PromptSender {
    /// `facts` carries the confirmed values as structures, so the output-integrity
    /// guard verifies attribution against the data itself rather than against the
    /// prompt text (which also carries news and the model's own frame).
    async fn send_prompt(
        &self,
        config: &Config,
        prompt: &str,
        facts: Option<&crate::integrity::ConfirmedFactSet>,
    ) -> Result<()>;
}
