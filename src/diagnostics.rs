//! The Diagnostic stream: the `tracing` subscriber that carries library
//! warnings to the operator's terminal.
//!
//! Not a contract (see the "Diagnostic stream" glossary entry in
//! `CONTEXT.md`): the events, their levels, and their wording may change at
//! any time. Only the `AEGIS_LOG` environment variable and a handful of named
//! message constants (e.g. `aegis_snapshot::SNAPSHOT_FAILED_CONTINUING`) are
//! contractual. It never substitutes for the append-only Audit log.
//!
//! Installing this subscriber must never fail the process: Aegis is a
//! guardrail, not the thing it guards, so a logging failure here is silently
//! ignored rather than propagated.

use std::fmt;
use std::io;

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

/// The environment variable that overrides the base level derived from the
/// CLI verbosity flags. The name and its override semantics are contractual;
/// see `docs/troubleshooting.md`.
pub const AEGIS_LOG_ENV_VAR: &str = "AEGIS_LOG";

/// Field values longer than this are cut at a UTF-8 character boundary and
/// marked with a trailing `... [truncated]`. Guards against an unbounded
/// value (a subprocess's verbatim stderr, for one) blowing up one log line.
const FIELD_TRUNCATION_LIMIT: usize = 512;

/// The base level implied by the CLI's `--verbosity` / `--quiet` / `-v`
/// flags, before an `AEGIS_LOG` override is applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BaseLevel {
    /// `--quiet` / `--verbosity quiet`
    Error,
    /// the default
    Warn,
    /// `--verbose` / `-v` / `--verbosity verbose`
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

/// Install the process-wide `tracing` subscriber that writes the Diagnostic
/// stream to stderr.
///
/// `base_level` is the level implied by the resolved CLI verbosity flags.
/// Setting `AEGIS_LOG` overrides it entirely, parsed as an `EnvFilter`
/// directive string; `RUST_LOG` is never read; Aegis runs inside other
/// people's environments, and that variable is usually set there for an
/// unrelated project.
///
/// `silent_unless_requested` keeps the stream off by default on a surface
/// whose stderr is byte-pinned by tests and never read by a human (the
/// internal language-worker mode, and the `hook` subcommand). Setting
/// `AEGIS_LOG` turns it on there too.
pub fn init(base_level: BaseLevel, silent_unless_requested: bool) {
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

    // A guardrail must not die because its own logging could not be
    // installed (for example, a test process that already set a default).
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// Compact event format: `aegis: <level>: <message> <field>=<value> ...`.
/// No timestamp and no target, both of which would make byte-pinned stderr
/// assertions nondeterministic across machines and refactors.
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

/// A `FormatFields` implementation that truncates every field value at
/// [`FIELD_TRUNCATION_LIMIT`] bytes before writing it. Applied once here so a
/// future `tracing::warn!` call anywhere in the workspace inherits the bound
/// automatically, rather than every emission site having to remember it.
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
        // `%e`-style Display captures and genuine Debug captures both arrive
        // here; either can carry an unbounded value (a subprocess's verbatim
        // stderr, in crates/aegis-snapshot/src/docker/mod.rs), so both go
        // through the same truncation path.
        self.write_field(field, &format!("{value:?}"));
    }
}

/// Cut `value` at [`FIELD_TRUNCATION_LIMIT`] bytes on a UTF-8 character
/// boundary and mark the cut, so a value straddling a multi-byte character
/// never panics and the operator can tell the line was shortened.
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
        // Each 'é' is 2 bytes, so a naive byte-index cut at the limit would
        // land inside the last character and panic on the slice.
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
}
