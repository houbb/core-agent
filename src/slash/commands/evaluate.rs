//! `/evaluate` — 任务质量评估
//!
//! 评价一次 Agent 任务质量，多维度评分。
//!
//! 用法：
//!   /evaluate <trace-id>            — 评估指定 Trace
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};
use crate::observability::{SqliteTraceStore, EvaluationEngine, Evaluation};

pub struct EvaluateCommand {
    store: std::sync::Arc<SqliteTraceStore>,
}

impl EvaluateCommand {
    pub fn new(store: std::sync::Arc<SqliteTraceStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl SlashCommand for EvaluateCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "evaluate".into(),
            display_name: "Task Evaluation".into(),
            description: "Evaluate Agent task quality with multi-dimension scoring".into(),
            usage: "/evaluate <trace-id>".into(),
            category: SlashCategory::Observability,
            min_args: 1,
            max_args: 1,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Observability
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let id = ctx.args.first().ok_or_else(|| {
            SlashError::InvalidArgument("usage: /evaluate <trace-id>".into())
        })?;

        let uuid = uuid::Uuid::parse_str(id).map_err(|_| {
            SlashError::InvalidArgument("invalid trace UUID format".into())
        })?;

        let trace = self
            .store
            .get_trace(uuid)
            .map_err(|e| SlashError::Execution(e))?
            .ok_or_else(|| SlashError::InvalidArgument(format!("Trace not found: {id}")))?;

        let eval = EvaluationEngine::evaluate(&trace);

        // 保存评估结果
        if let Err(e) = self.store.save_evaluation(&eval) {
            return Err(SlashError::Execution(format!("cannot save evaluation: {e}")));
        }

        let mut output = String::new();
        output.push_str("Evaluation Result\n\n");
        output.push_str(&format!("Task: {}\n\n", trace.goal));
        output.push_str(&format!("Score: {:.1} / 10\n\n", eval.overall));
        output.push_str("Criteria:\n\n");

        for c in &eval.criteria {
            let bar = score_bar(c.score, c.max_score);
            output.push_str(&format!(
                "  {}  {:.1}  {}\n",
                c.dimension.as_str(),
                c.score,
                bar
            ));
        }

        output.push_str(&format!("\nFeedback:\n  {}\n", eval.feedback));

        Ok(CommandOutput::new(output))
    }
}

fn score_bar(score: f32, max: f32) -> String {
    let ratio = (score / max).clamp(0.0, 1.0);
    let filled = (ratio * 10.0).round() as usize;
    let empty = 10 - filled;
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}