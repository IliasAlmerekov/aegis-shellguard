; Python call-expression captures (ADR-022 §3).
; Structural capture only; semantic interpretation lives in `python.rs`.

(call
  function: (attribute
    object: (identifier) @obj
    attribute: (identifier) @attr)
  arguments: (argument_list) @args) @call

(call
  function: (identifier) @fname
  arguments: (argument_list) @args) @call