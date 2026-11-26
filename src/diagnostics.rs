use std::fmt;
use std::io;

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

use crate::shell_compat::InvocationMode;
use crate::{Commands, OutputVerbosity};

pub const AEGIS_LOG_ENV_VAR: &str = "AEGIS_LOG";

const FIELD_TRUNCATION_LIMIT: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BaseLevel {
    Error,
    Warn,
    Info,
}

impl BaseLevel {
    fn as_filter_str(self) -> &'static str {
        match self {
            BaseLevel::Error => "error",
            BaseLevel::Warn => "warn",
            BaseLevel::Info => "info",
        }
    }
}

pub fn init(invocation: &InvocationMode) {
    let (base_level, silent_unless_requested) = derive_settings(invocation);
    install(base_level, silent_unless_requested);
}

fn derive_settings(invocation: &InvocationMode) -> (BaseLevel, bool) {
    let base_level = match invocation {
        InvocationMode::Cli(cli) => {
            match OutputVerbosity::from_cli(cli.verbosity, cli.quiet, cli.verbose) {
                OutputVerbosity::Quiet => BaseLevel::Error,
                OutputVerbosity::Standard => BaseLevel::Warn,
                OutputVerbosity::Verbose => BaseLevel::Info,
            }
        }
        _ => BaseLevel::Warn,
    };
    let silent_unless_requested = matches!(
        invocation,
        InvocationMode::Cli(cli) if matches!(cli.subcommand, Some(Commands::Hook))
    );
    (base_level, silent_unless_requested)
}

fn install(base_level: BaseLevel, silent_unless_requested: bool) {
    let aegis_log_is_set = std::env::var_os(AEGIS_LOG_ENV_VAR).is_some();

    if silent_unless_requested && !aegis_log_is_set {
        return;
    }

    let filter = if aegis_log_is_set {
        EnvFilter::try_from_env(AEGIS_LOG_ENV_VAR)
            .unwrap_or_else(|_| EnvFilter::new(base_level.as_filter_str()))
    } else {
        EnvFilter::new(base_level.as_filter_str())
    };

    let subscriber = tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(filter)
        .fmt_fields(TruncatingFields)
        .event_format(AegisEventFormat)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}

struct AegisEventFormat;

impl<S, N> FormatEvent<S, N> for AegisEventFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        write!(
            writer,
            "aegis: {}: ",
            event.metadata().level().to_string().to_lowercase()
        )?;
        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

struct TruncatingFields;

impl<'writer> FormatFields<'writer> for TruncatingFields {
    fn format_fields<R: RecordFields>(&self, writer: Writer<'writer>, fields: R) -> fmt::Result {
        let mut visitor = TruncatingVisitor::new(writer);
        fields.record(&mut visitor);
        visitor.finish()
    }
}

struct TruncatingVisitor<'a> {
    writer: Writer<'a>,
    wrote_any: bool,
    result: fmt::Result,
}

impl<'a> TruncatingVisitor<'a> {
    fn new(writer: Writer<'a>) -> Self {
        Self {
            writer,
            wrote_any: false,
            result: Ok(()),
        }
    }

    fn write_field(&mut self, field: &Field, value: &str) {
        if self.result.is_err() {
            return;
        }
        let value = truncate_field(value);
        let separator = if self.wrote_any { " " } else { "" };
        self.wrote_any = true;
        self.result = if field.name() == "message" {
            write!(self.writer, "{separator}{value}")
        } else {
            write!(self.writer, "{separator}{}={value}", field.name())
        };
    }

    fn finish(self) -> fmt::Result {
        self.result
    }
}

impl Visit for TruncatingVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.write_field(field, value);
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.write_field(field, &format!("{value:?}"));
    }
}

fn truncate_field(value: &str) -> std::borrow::Cow<'_, str> {
    if value.len() <= FIELD_TRUNCATION_LIMIT {
        return std::borrow::Cow::Borrowed(value);
    }
    let mut boundary = FIELD_TRUNCATION_LIMIT;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let mut truncated = String::with_capacity(boundary + 16);
    truncated.push_str(&value[..boundary]);
    truncated.push_str(" ... [truncated]");
    std::borrow::Cow::Owned(truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cli, CommandOutputFormat};

    #[test]
    fn truncate_field_leaves_short_values_untouched() {
        assert_eq!(truncate_field("short"), "short");
    }

    #[test]
    fn truncate_field_cuts_long_values_and_marks_the_cut() {
        let long_value = "a".repeat(FIELD_TRUNCATION_LIMIT + 100);
        let truncated = truncate_field(&long_value);
        assert!(truncated.len() < long_value.len());
        assert!(truncated.ends_with("... [truncated]"));
    }

    #[test]
    fn truncate_field_cuts_on_a_char_boundary() {
        let long_value = "é".repeat(FIELD_TRUNCATION_LIMIT);
        let truncated = truncate_field(&long_value);
        assert!(truncated.ends_with("... [truncated]"));
    }

    #[test]
    fn base_level_maps_to_expected_filter_directives() {
        assert_eq!(BaseLevel::Error.as_filter_str(), "error");
        assert_eq!(BaseLevel::Warn.as_filter_str(), "warn");
        assert_eq!(BaseLevel::Info.as_filter_str(), "info");
    }

    fn cli(verbosity: OutputVerbosity, subcommand: Option<Commands>) -> Cli {
        Cli {
            command: None,
            output: CommandOutputFormat::Text,
            verbosity,
            quiet: false,
            verbose: false,
            subcommand,
        }
    }

    #[test]
    fn quiet_verbosity_derives_error_level() {
        let invocation = InvocationMode::Cli(cli(OutputVerbosity::Quiet, None));
        assert_eq!(derive_settings(&invocation).0, BaseLevel::Error);
    }

    #[test]
    fn standard_verbosity_derives_warn_level() {
        let invocation = InvocationMode::Cli(cli(OutputVerbosity::Standard, None));
        assert_eq!(derive_settings(&invocation).0, BaseLevel::Warn);
    }

    #[test]
    fn verbose_verbosity_derives_info_level() {
        let invocation = InvocationMode::Cli(cli(OutputVerbosity::Verbose, None));
        assert_eq!(derive_settings(&invocation).0, BaseLevel::Info);
    }

    #[test]
    fn hook_subcommand_is_silent_by_default() {
        let invocation = InvocationMode::Cli(cli(OutputVerbosity::Standard, Some(Commands::Hook)));
        assert!(derive_settings(&invocation).1);
    }

    #[test]
    fn other_subcommands_are_not_silent_by_default() {
        let invocation = InvocationMode::Cli(cli(OutputVerbosity::Standard, Some(Commands::On)));
        assert!(!derive_settings(&invocation).1);
    }

    #[test]
    fn no_subcommand_is_not_silent_by_default() {
        let invocation = InvocationMode::Cli(cli(OutputVerbosity::Standard, None));
        assert!(!derive_settings(&invocation).1);
    }
}
