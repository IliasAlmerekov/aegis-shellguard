; Bash command and file-redirect captures (ADR-022 §3).
; Structural capture only; semantic interpretation lives in `bash.rs`.
; `heredoc_redirect` is input and `heredoc_body` is routed separately by the
; orchestration layer, so neither is captured here.

(command
  name: (command_name) @name) @cmd

(file_redirect) @redirect