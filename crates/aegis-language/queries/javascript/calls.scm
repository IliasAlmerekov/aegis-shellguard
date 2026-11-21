; JavaScript call- and new-expression captures (ADR-022 §3).
; Structural capture only; semantic interpretation lives in `javascript.rs` / `family.rs`.

(call_expression
  function: (member_expression
    object: (identifier) @obj
    property: (property_identifier) @attr)
  arguments: (arguments) @args) @call

(call_expression
  function: (identifier) @fname
  arguments: (arguments) @args) @call

(new_expression
  constructor: (identifier) @fname
  arguments: (arguments) @args) @call