use crate::allowlist::ConfigSourceLayer;

use super::warning::{RatchetSink, is_project};

pub(crate) trait Ratchet<T> {
    #[allow(clippy::too_many_arguments)]
    fn merge(
        &self,
        path: &'static str,
        base: T,
        overlay: Option<T>,
        layer: ConfigSourceLayer,
        location: &str,
        sink: &mut RatchetSink,
    ) -> T;
}

pub(crate) struct Tighten<T> {
    stricter: fn(T, T) -> T,
    format: fn(&T) -> String,
    ceiling: Option<fn(T) -> T>,
}

impl<T> Tighten<T> {
    pub(crate) fn new(stricter: fn(T, T) -> T, format: fn(&T) -> String) -> Self {
        Self {
            stricter,
            format,
            ceiling: None,
        }
    }

    pub(crate) fn with_ceiling(
        stricter: fn(T, T) -> T,
        format: fn(&T) -> String,
        ceiling: fn(T) -> T,
    ) -> Self {
        Self {
            stricter,
            format,
            ceiling: Some(ceiling),
        }
    }
}

impl<T: Clone> Ratchet<T> for Tighten<T> {
    fn merge(
        &self,
        path: &'static str,
        base: T,
        overlay: Option<T>,
        layer: ConfigSourceLayer,
        location: &str,
        sink: &mut RatchetSink,
    ) -> T {
        sink.touch(path);
        let Some(requested) = overlay else {
            return base;
        };
        let clamped = match self.ceiling {
            Some(clamp) => clamp(requested.clone()),
            None => requested.clone(),
        };
        let kept = match layer {
            ConfigSourceLayer::Global => clamped,
            ConfigSourceLayer::Project => (self.stricter)(base.clone(), clamped),
        };
        if is_project(layer) {
            sink.warn(
                path,
                (self.format)(&requested),
                (self.format)(&kept),
                location,
            );
        }
        kept
    }
}

pub(crate) struct GlobalOnly<T> {
    format: fn(&T) -> String,
}

impl<T> GlobalOnly<T> {
    pub(crate) fn new(format: fn(&T) -> String) -> Self {
        Self { format }
    }
}

impl<T: Clone> Ratchet<T> for GlobalOnly<T> {
    fn merge(
        &self,
        path: &'static str,
        base: T,
        overlay: Option<T>,
        layer: ConfigSourceLayer,
        location: &str,
        sink: &mut RatchetSink,
    ) -> T {
        sink.touch(path);
        match layer {
            ConfigSourceLayer::Global => overlay.unwrap_or(base),
            ConfigSourceLayer::Project => {
                if let Some(requested) = &overlay {
                    sink.warn(
                        path,
                        (self.format)(requested),
                        (self.format)(&base),
                        location,
                    );
                }
                base
            }
        }
    }
}

pub(crate) struct Unratcheted<T> {
    _value: std::marker::PhantomData<T>,
}

impl<T> Unratcheted<T> {
    pub(crate) fn new() -> Self {
        Self {
            _value: std::marker::PhantomData,
        }
    }
}

impl<T> Ratchet<T> for Unratcheted<T> {
    fn merge(
        &self,
        path: &'static str,
        base: T,
        overlay: Option<T>,
        _layer: ConfigSourceLayer,
        _location: &str,
        sink: &mut RatchetSink,
    ) -> T {
        sink.touch(path);
        overlay.unwrap_or(base)
    }
}

pub(crate) fn append<T>(base: Vec<T>, overlay: Vec<T>) -> Vec<T> {
    let mut merged = base;
    merged.extend(overlay);
    merged
}

pub(crate) fn format_debug<T: std::fmt::Debug>(value: &T) -> String {
    format!("{value:?}")
}

pub(crate) fn format_display<T: std::fmt::Display>(value: &T) -> String {
    value.to_string()
}

pub(crate) fn bool_true_is_stricter(base: bool, requested: bool) -> bool {
    base || requested
}

pub(crate) fn bool_false_is_stricter(base: bool, requested: bool) -> bool {
    base && requested
}

pub(crate) fn keep_larger<T: Ord>(base: T, requested: T) -> T {
    requested.max(base)
}

pub(crate) fn keep_smaller<T: Ord>(base: T, requested: T) -> T {
    requested.min(base)
}
