use super::*;

#[test]
fn effective_token_slices_strip_poetry_run_before_interpreter() {
    let tokens = ["poetry", "run", "python3", "./script.py"];
    let slices = effective_token_slices(&tokens);

    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].program, "python3");
    assert_eq!(slices[0].tokens, vec!["python3", "./script.py"]);
}

#[test]
fn effective_token_slices_strip_pipenv_run_before_interpreter() {
    let tokens = ["pipenv", "run", "python3", "./script.py"];
    let slices = effective_token_slices(&tokens);

    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].program, "python3");
    assert_eq!(slices[0].tokens, vec!["python3", "./script.py"]);
}

#[test]
fn effective_token_slices_strip_uv_run_option_terminator() {
    let tokens = ["uv", "run", "--", "python3", "./script.py"];
    let slices = effective_token_slices(&tokens);

    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].program, "python3");
    assert_eq!(slices[0].tokens, vec!["python3", "./script.py"]);
}

#[test]
fn effective_token_slices_strip_uv_tool_run_prefix() {
    // `uv tool run` is the long form of `uvx` (#421, ADR-040): it must strip
    // the same three-token prefix as `uv run` strips two.
    let tokens = ["uv", "tool", "run", "python3", "./script.py"];
    let slices = effective_token_slices(&tokens);

    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].program, "python3");
    assert_eq!(slices[0].tokens, vec!["python3", "./script.py"]);
}

#[test]
fn effective_token_slices_strip_path_qualified_uv_tool_run_prefix() {
    let tokens = ["/usr/bin/uv", "tool", "run", "python3", "./script.py"];
    let slices = effective_token_slices(&tokens);

    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].program, "python3");
    assert_eq!(slices[0].tokens, vec!["python3", "./script.py"]);
}

#[test]
fn pipx_run_selects_a_package_executable_for_a_non_script_operand() {
    assert!(Runner::Pipx.selects_package_executable(&["pipx", "run", "black"]));
}

#[test]
fn uv_run_script_forces_python_script_but_uv_run_alone_does_not() {
    assert!(Runner::Uv.forces_python_script(&["uv", "run", "--script", "./danger"]));
    assert!(!Runner::Uv.forces_python_script(&["uv", "run", "./danger"]));
    assert!(!Runner::Pipx.forces_python_script(&["pipx", "run", "--script", "./danger"]));
}
