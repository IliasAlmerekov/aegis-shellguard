//! `AegisConfig::merge_layer` — the exhaustive one-layer merge (ADR-013,
//! CONTEXT.md "Ratchet direction"). Split out of `model.rs` to stay under the
//! 800-line file budget; the rank comparators it uses as `Tighten` `stricter`
//! functions (`most_restrictive_mode` and friends) stay in `model.rs`, which
//! this module is a child of and can see them without qualification.

use super::partial::PartialConfig;
use super::ratchet::{self, Ratchet, RatchetContext, RatchetSink, SecurityRatchetWarning};
use super::{
    AegisConfig, ConfigSourceLayer, most_restrictive_allowlist_override_level,
    most_restrictive_ci_policy, most_restrictive_mode, most_restrictive_snapshot_policy,
};

impl AegisConfig {
    /// Merge one config layer into `base`.
    ///
    /// Both structs are destructured exhaustively, field by field, with no
    /// `..` — adding a field to `AegisConfig` or a nested config struct
    /// without routing it through a `Ratchet` direction is a compile error,
    /// not a silent last-wins default. Each field's direction is declared at
    /// its call site here or in the `ratchet::merge_*`/`ratchet::custom_*`
    /// function it delegates to for a whole nested struct.
    pub(super) fn merge_layer(
        base: Self,
        overlay: PartialConfig,
        layer: ConfigSourceLayer,
        location: &str,
    ) -> (Self, Vec<SecurityRatchetWarning>) {
        let mut sink = RatchetSink::default();
        // Provider-target `Custom` rules need to know whether the base
        // already enabled each provider, computed BEFORE any field below
        // moves out of `base` (#269 C3-01).
        let ctx = RatchetContext::compute(&base);

        let Self {
            config_version: base_config_version,
            mode: base_mode,
            custom_patterns: base_custom_patterns,
            custom_pattern_layers: base_custom_pattern_layers,
            allowlist: base_allowlist,
            allowlist_layers: base_allowlist_layers,
            blocklist: base_blocklist,
            blocklist_layers: base_blocklist_layers,
            audit_max_file_size_bytes_source: base_audit_max_file_size_bytes_source,
            audit_retention_files_source: base_audit_retention_files_source,
            allowlist_override_level: base_allowlist_override_level,
            snapshot_policy: base_snapshot_policy,
            auto_snapshot_git: base_auto_snapshot_git,
            auto_snapshot_docker: base_auto_snapshot_docker,
            auto_snapshot_postgres: base_auto_snapshot_postgres,
            postgres_snapshot: base_postgres_snapshot,
            auto_snapshot_mysql: base_auto_snapshot_mysql,
            mysql_snapshot: base_mysql_snapshot,
            auto_snapshot_supabase: base_auto_snapshot_supabase,
            supabase_snapshot: base_supabase_snapshot,
            auto_snapshot_sqlite: base_auto_snapshot_sqlite,
            sqlite_snapshot_path: base_sqlite_snapshot_path,
            docker_scope: base_docker_scope,
            ci_policy: base_ci_policy,
            audit: base_audit,
            rules: base_rules,
            sandbox: base_sandbox,
            prune: base_prune,
            language_analysis: base_language_analysis,
        } = base;

        let PartialConfig {
            config_version: req_config_version,
            mode: req_mode,
            custom_patterns: req_custom_patterns,
            allowlist: req_allowlist,
            blocklist: req_blocklist,
            allowlist_override_level: req_allowlist_override_level,
            snapshot_policy: req_snapshot_policy,
            auto_snapshot_git: req_auto_snapshot_git,
            auto_snapshot_docker: req_auto_snapshot_docker,
            auto_snapshot_postgres: req_auto_snapshot_postgres,
            postgres_snapshot: req_postgres_snapshot,
            auto_snapshot_mysql: req_auto_snapshot_mysql,
            mysql_snapshot: req_mysql_snapshot,
            auto_snapshot_supabase: req_auto_snapshot_supabase,
            supabase_snapshot: req_supabase_snapshot,
            auto_snapshot_sqlite: req_auto_snapshot_sqlite,
            sqlite_snapshot_path: req_sqlite_snapshot_path,
            docker_scope: req_docker_scope,
            ci_policy: req_ci_policy,
            audit: req_audit,
            rules: req_rules,
            sandbox: req_sandbox,
            prune: req_prune,
            language_analysis: req_language_analysis,
        } = overlay;

        // Provenance bookkeeping peeks at the raw requested retention values
        // (both `Option<T>` and `Copy`) before `req_audit` moves into
        // `ratchet::merge_audit` below.
        let req_max_file_size_bytes = req_audit.max_file_size_bytes;
        let req_retention_files = req_audit.retention_files;

        let custom_pattern_count = req_custom_patterns.len();
        let allowlist_count = req_allowlist.len();
        let blocklist_count = req_blocklist.len();

        let custom_patterns = ratchet::append(base_custom_patterns, req_custom_patterns);
        let mut custom_pattern_layers = base_custom_pattern_layers;
        custom_pattern_layers.extend(std::iter::repeat_n(layer, custom_pattern_count));

        let allowlist = ratchet::append(base_allowlist, req_allowlist);
        let mut allowlist_layers = base_allowlist_layers;
        allowlist_layers.extend(std::iter::repeat_n(layer, allowlist_count));

        let blocklist = ratchet::append(base_blocklist, req_blocklist);
        let mut blocklist_layers = base_blocklist_layers;
        blocklist_layers.extend(std::iter::repeat_n(layer, blocklist_count));

        let mode = ratchet::Tighten::new(most_restrictive_mode, ratchet::format_debug)
            .merge("mode", base_mode, req_mode, layer, location, &mut sink);
        let allowlist_override_level = ratchet::Tighten::new(
            most_restrictive_allowlist_override_level,
            ratchet::format_debug,
        )
        .merge(
            "allowlist_override_level",
            base_allowlist_override_level,
            req_allowlist_override_level,
            layer,
            location,
            &mut sink,
        );
        let snapshot_policy =
            ratchet::Tighten::new(most_restrictive_snapshot_policy, ratchet::format_debug).merge(
                "snapshot_policy",
                base_snapshot_policy,
                req_snapshot_policy,
                layer,
                location,
                &mut sink,
            );
        let ci_policy = ratchet::Tighten::new(most_restrictive_ci_policy, ratchet::format_debug)
            .merge(
                "ci_policy",
                base_ci_policy,
                req_ci_policy,
                layer,
                location,
                &mut sink,
            );

        let auto_snapshot_git =
            ratchet::Tighten::new(ratchet::bool_true_is_stricter, ratchet::format_display).merge(
                "auto_snapshot_git",
                base_auto_snapshot_git,
                req_auto_snapshot_git,
                layer,
                location,
                &mut sink,
            );
        let auto_snapshot_docker =
            ratchet::Tighten::new(ratchet::bool_true_is_stricter, ratchet::format_display).merge(
                "auto_snapshot_docker",
                base_auto_snapshot_docker,
                req_auto_snapshot_docker,
                layer,
                location,
                &mut sink,
            );
        let auto_snapshot_postgres =
            ratchet::Tighten::new(ratchet::bool_true_is_stricter, ratchet::format_display).merge(
                "auto_snapshot_postgres",
                base_auto_snapshot_postgres,
                req_auto_snapshot_postgres,
                layer,
                location,
                &mut sink,
            );
        let auto_snapshot_mysql =
            ratchet::Tighten::new(ratchet::bool_true_is_stricter, ratchet::format_display).merge(
                "auto_snapshot_mysql",
                base_auto_snapshot_mysql,
                req_auto_snapshot_mysql,
                layer,
                location,
                &mut sink,
            );
        let auto_snapshot_supabase =
            ratchet::Tighten::new(ratchet::bool_true_is_stricter, ratchet::format_display).merge(
                "auto_snapshot_supabase",
                base_auto_snapshot_supabase,
                req_auto_snapshot_supabase,
                layer,
                location,
                &mut sink,
            );
        let auto_snapshot_sqlite =
            ratchet::Tighten::new(ratchet::bool_true_is_stricter, ratchet::format_display).merge(
                "auto_snapshot_sqlite",
                base_auto_snapshot_sqlite,
                req_auto_snapshot_sqlite,
                layer,
                location,
                &mut sink,
            );

        let postgres_snapshot = ratchet::custom_postgres_snapshot(
            base_postgres_snapshot,
            req_postgres_snapshot,
            layer,
            location,
            ctx.postgres_enabled,
            &mut sink,
        );
        let mysql_snapshot = ratchet::custom_mysql_snapshot(
            base_mysql_snapshot,
            req_mysql_snapshot,
            layer,
            location,
            ctx.mysql_enabled,
            &mut sink,
        );
        let supabase_snapshot = ratchet::custom_supabase_snapshot(
            base_supabase_snapshot,
            req_supabase_snapshot,
            layer,
            location,
            ctx.supabase_enabled,
            &mut sink,
        );
        let sqlite_snapshot_path = ratchet::custom_sqlite_snapshot(
            base_sqlite_snapshot_path,
            req_sqlite_snapshot_path,
            layer,
            location,
            ctx.sqlite_enabled,
            &mut sink,
        );
        let docker_scope = ratchet::custom_docker_scope(
            base_docker_scope,
            req_docker_scope,
            layer,
            location,
            ctx.docker_enabled,
            &mut sink,
        );

        let rules = ratchet::custom_rules(base_rules, req_rules, layer, location, &mut sink);

        let audit = ratchet::merge_audit(base_audit, req_audit, layer, location, &mut sink);
        let audit_max_file_size_bytes_source =
            if req_max_file_size_bytes == Some(audit.max_file_size_bytes) {
                Some(layer)
            } else {
                base_audit_max_file_size_bytes_source
            };
        let audit_retention_files_source = if req_retention_files == Some(audit.retention_files) {
            Some(layer)
        } else {
            base_audit_retention_files_source
        };

        let sandbox = ratchet::merge_sandbox(base_sandbox, req_sandbox, layer, location, &mut sink);
        let prune = ratchet::merge_prune(base_prune, req_prune, layer, location, &mut sink);
        let language_analysis = ratchet::merge_language_analysis(
            base_language_analysis,
            req_language_analysis,
            layer,
            location,
            &mut sink,
        );

        let config_version = ratchet::Unratcheted::new("schema version, not a security posture")
            .merge(
                "config_version",
                base_config_version,
                req_config_version,
                layer,
                location,
                &mut sink,
            );

        let config = Self {
            config_version,
            mode,
            custom_patterns,
            custom_pattern_layers,
            allowlist,
            allowlist_layers,
            blocklist,
            blocklist_layers,
            audit_max_file_size_bytes_source,
            audit_retention_files_source,
            allowlist_override_level,
            snapshot_policy,
            auto_snapshot_git,
            auto_snapshot_docker,
            auto_snapshot_postgres,
            postgres_snapshot,
            auto_snapshot_mysql,
            mysql_snapshot,
            auto_snapshot_supabase,
            supabase_snapshot,
            auto_snapshot_sqlite,
            sqlite_snapshot_path,
            docker_scope,
            ci_policy,
            audit,
            rules,
            sandbox,
            prune,
            language_analysis,
        };

        (config, sink.into_warnings())
    }
}
