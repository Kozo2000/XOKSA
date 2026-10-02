use crate::config::Config;
use crate::fundamental::FundamentalData;
use crate::llm;
use crate::news::Article;
use crate::render::{Renderer, TerminalRenderer};
use crate::technical::calculate_final_score_snapshot;
use crate::technical::TechnicalDataGuard;

pub trait PromptRenderer {
    fn build_prompt(
        &self,
        config: &Config,
        guard: &TechnicalDataGuard,
        news_articles: Option<&[Article]>,
        fundamental_data: Option<&FundamentalData>,
    ) -> String;
}

pub struct DefaultPromptRenderer<R: Renderer> {
    renderer: R,
}

impl<R: Renderer> DefaultPromptRenderer<R> {
    pub fn new(renderer: R) -> Self {
        Self { renderer }
    }
}

impl<R: Renderer> PromptRenderer for DefaultPromptRenderer<R> {
    fn build_prompt(
        &self,
        config: &Config,
        guard: &TechnicalDataGuard,
        news_articles: Option<&[Article]>,
        fundamental_data: Option<&FundamentalData>,
    ) -> String {
        // Weight-ranked indicator blocks (basic folded in): one ordering shared
        // with the terminal display and `/show technical` (render::render_ranked).
        let indicator_sections: Vec<Vec<String>> = crate::render::render_ranked(config, guard)
            .into_iter()
            .map(|result| result.description)
            .collect();

        let snap = calculate_final_score_snapshot(config, guard);
        let score_lines =
            self.renderer
                .compose_final_score_lines(&snap, &config.stance, true, &config.lang);

        let lines = llm::compose_llm_prompt_lines(
            config,
            guard,
            news_articles,
            &[],
            &indicator_sections,
            &score_lines,
            fundamental_data,
        );
        lines.join("\n")
    }
}

pub fn build_analysis_prompt(
    config: &Config,
    guard: &TechnicalDataGuard,
    news_articles: Option<&[Article]>,
    fundamental_data: Option<&FundamentalData>,
) -> String {
    DefaultPromptRenderer::new(TerminalRenderer).build_prompt(
        config,
        guard,
        news_articles,
        fundamental_data,
    )
}
