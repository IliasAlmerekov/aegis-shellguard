use std::cell::Cell;

thread_local! {
    static REGISTRY_BUILD_COUNT: Cell<usize> = const { Cell::new(0) };
}

pub fn increment_registry_build_count() {
    REGISTRY_BUILD_COUNT.with(|c| c.set(c.get() + 1));
}

pub fn registry_build_count() -> usize {
    REGISTRY_BUILD_COUNT.with(Cell::get)
}

pub fn reset_registry_build_count() {
    REGISTRY_BUILD_COUNT.with(|c| c.set(0));
}
