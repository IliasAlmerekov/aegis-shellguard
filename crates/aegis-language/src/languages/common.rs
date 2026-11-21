use tree_sitter::Node;

use crate::operation::ByteSpan;

pub(crate) fn span_for(node: Node) -> ByteSpan {
    let start = node.start_position();
    ByteSpan {
        line: start.row as u32 + 1,
        column: start.column as u32 + 1,
        byte_start: node.start_byte(),
        byte_end: node.end_byte(),
    }
}

#[cfg(test)]
mod tests {
    use tree_sitter::Parser;

    use super::span_for;
    use crate::language::SourceLanguage;
    use crate::operation::ByteSpan;

    fn parse_python(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&SourceLanguage::Python.tree_sitter_language())
            .expect("pinned python grammar is ABI-compatible");
        parser
            .parse(source.as_bytes(), None)
            .expect("source parses")
    }

    #[test]
    fn span_for_reports_one_based_line_and_column_at_start_of_source() {
        let source = "os.remove(\"a\")";
        let tree = parse_python(source);
        let stmt = tree
            .root_node()
            .named_child(0)
            .expect("expression statement");

        assert_eq!(
            span_for(stmt),
            ByteSpan {
                line: 1,
                column: 1,
                byte_start: 0,
                byte_end: source.len(),
            }
        );
    }

    #[test]
    fn span_for_reports_one_based_line_and_column_on_indented_later_line() {
        let source = "# header\n\n    os.remove(\"a\")\n";
        let tree = parse_python(source);
        let call_stmt = tree.root_node().named_child(1).expect("second statement");

        assert_eq!(
            span_for(call_stmt),
            ByteSpan {
                line: 3,
                column: 5,
                byte_start: 14,
                byte_end: 28,
            }
        );
    }
}
