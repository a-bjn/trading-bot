use anyhow::Result;
use std::path::PathBuf;
use tracing::Level;
use tracing_subscriber::{
    layer::SubscriberExt,
    filter::FilterFn,
    Layer,
    EnvFilter,
    fmt::{self, time::ChronoUtc},
    registry::Registry,
};
use tracing_appender::{
    non_blocking,
    rolling::{RollingFileAppender, Rotation},
};
use serde_json::json;

/// Setup production-grade logging with file rotation and JSON structured logs
pub fn setup_logging(log_level: &str, log_to_file: bool, log_dir: &str) -> Result<LoggingGuard> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| log_level.into());

    // Console layer with human-readable format
    let console_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(true)
        .with_timer(ChronoUtc::rfc_3339())
        .with_writer(std::io::stdout);

    let guard = if log_to_file {
        // Create logs directory if it doesn't exist
        let log_path = PathBuf::from(log_dir);
        std::fs::create_dir_all(&log_path)?;

        // General log file with rotation (daily rotation, keep 30 days)
        let general_file_appender = RollingFileAppender::new(
            Rotation::DAILY,
            &log_path,
            "trading-bot.log",
        );
        let (general_writer, general_guard) = non_blocking(general_file_appender);

        // Error log file (separate file for errors only)
        let error_file_appender = RollingFileAppender::new(
            Rotation::DAILY,
            &log_path,
            "trading-bot-errors.log",
        );
        let (error_writer, error_guard) = non_blocking(error_file_appender);

        // JSON structured log layer for general logs
        let json_layer = fmt::layer()
            .json()
            .with_writer(general_writer)
            .with_current_span(false)
            .with_span_list(false);

        // Error log layer (human-readable for easier debugging)
        let error_layer = fmt::layer()
            .with_target(true)
            .with_level(true)
            .with_thread_ids(false)
            .with_file(true)
            .with_line_number(true)
            .with_timer(ChronoUtc::rfc_3339())
            .with_writer(error_writer);

        // Create filter for error layer
        let error_filter = FilterFn::new(|metadata| {
            metadata.level() == &Level::ERROR
        });

        // Build the subscriber with all layers
        let subscriber = Registry::default()
            .with(filter)
            .with(console_layer)
            .with(json_layer)
            .with(error_layer.with_filter(error_filter));

        tracing::subscriber::set_global_default(subscriber)?;

        LoggingGuard {
            _general_guard: Some(general_guard),
            _error_guard: Some(error_guard),
        }
    } else {
        // Build the subscriber with only console layer
        let subscriber = Registry::default()
            .with(filter)
            .with(console_layer);

        tracing::subscriber::set_global_default(subscriber)?;

        LoggingGuard {
            _general_guard: None,
            _error_guard: None,
        }
    };

    tracing::info!("Logging initialized - level: {}, file logging: {}", log_level, log_to_file);

    Ok(guard)
}

/// Guard that keeps log file writers alive
pub struct LoggingGuard {
    _general_guard: Option<non_blocking::WorkerGuard>,
    _error_guard: Option<non_blocking::WorkerGuard>,
}

/// Log a structured trade event as JSON
pub fn log_trade_event(event: TradeEvent) {
    let json_event = json!({
        "event_type": "trade",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "trade": {
            "signature": event.signature,
            "token_mint": event.token_mint,
            "action": event.action,
            "amount_sol": event.amount_sol,
            "amount_tokens": event.amount_tokens,
            "entry_price": event.entry_price,
            "exit_price": event.exit_price,
            "profit_loss_sol": event.profit_loss_sol,
            "profit_loss_percentage": event.profit_loss_percentage,
            "execution_time_ms": event.execution_time_ms,
            "status": event.status,
        }
    });

    tracing::info!(trade_event = %json_event.to_string(), "Trade executed");
}

/// Structured trade event for logging
#[derive(Debug, Clone)]
pub struct TradeEvent {
    pub signature: String,
    pub token_mint: String,
    pub action: String, // "buy" or "sell"
    pub amount_sol: f64,
    pub amount_tokens: Option<f64>,
    pub entry_price: Option<f64>,
    pub exit_price: Option<f64>,
    pub profit_loss_sol: Option<f64>,
    pub profit_loss_percentage: Option<f64>,
    pub execution_time_ms: u64,
    pub status: String, // "success", "failed", "partial"
}

impl TradeEvent {
    pub fn buy(
        signature: String,
        token_mint: String,
        amount_sol: f64,
        amount_tokens: Option<f64>,
        entry_price: Option<f64>,
        execution_time_ms: u64,
        status: String,
    ) -> Self {
        Self {
            signature,
            token_mint,
            action: "buy".to_string(),
            amount_sol,
            amount_tokens,
            entry_price,
            exit_price: None,
            profit_loss_sol: None,
            profit_loss_percentage: None,
            execution_time_ms,
            status,
        }
    }

    pub fn sell(
        signature: String,
        token_mint: String,
        amount_tokens: f64,
        exit_price: Option<f64>,
        profit_loss_sol: Option<f64>,
        profit_loss_percentage: Option<f64>,
        execution_time_ms: u64,
        status: String,
    ) -> Self {
        Self {
            signature,
            token_mint,
            action: "sell".to_string(),
            amount_sol: 0.0,
            amount_tokens: Some(amount_tokens),
            entry_price: None,
            exit_price,
            profit_loss_sol,
            profit_loss_percentage,
            execution_time_ms,
            status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_event_buy() {
        let event = TradeEvent::buy(
            "test_sig".to_string(),
            "test_mint".to_string(),
            1.0,
            Some(1000.0),
            Some(0.001),
            100,
            "success".to_string(),
        );

        assert_eq!(event.action, "buy");
        assert_eq!(event.amount_sol, 1.0);
    }

    #[test]
    fn test_trade_event_sell() {
        let event = TradeEvent::sell(
            "test_sig".to_string(),
            "test_mint".to_string(),
            1000.0,
            Some(0.002),
            Some(1.0),
            Some(100.0),
            150,
            "success".to_string(),
        );

        assert_eq!(event.action, "sell");
        assert!(event.profit_loss_sol.is_some());
    }
}
