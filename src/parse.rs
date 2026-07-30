//! Thin wrapper over tree-sitter, plus the tree-walking helper every rule uses.

use anyhow::{Context, Result};
use tree_sitter::{Node, Parser, Tree};

/// Parse source text into a syntax tree.
pub fn parse(language: &tree_sitter::Language, src: &str) -> Result<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(language)
        .context("failed to set tree-sitter language (likely a grammar ABI mismatch)")?;
    parser
        .parse(src, None)
        .context("tree-sitter returned no tree")
}

/// `root` and everything under it, depth-first in document order. Nicer to write
/// checks against than raw cursors.
pub fn descendants<'t>(root: Node<'t>) -> Descendants<'t> {
    Descendants { stack: vec![root] }
}

pub struct Descendants<'t> {
    stack: Vec<Node<'t>>,
}

impl<'t> Iterator for Descendants<'t> {
    type Item = Node<'t>;

    fn next(&mut self) -> Option<Node<'t>> {
        let node = self.stack.pop()?;
        let mut cursor = node.walk();
        let children: Vec<Node<'t>> = node.children(&mut cursor).collect();
        // Push in reverse so that popping yields document order.
        for child in children.into_iter().rev() {
            self.stack.push(child);
        }
        Some(node)
    }
}

/// The source text a node spans.
pub fn text<'a>(node: Node<'_>, src: &'a str) -> &'a str {
    node.utf8_text(src.as_bytes()).unwrap_or("")
}

/// 1-based line number of a node's start, which is what editors and humans use.
pub fn line_of(node: Node<'_>) -> usize {
    node.start_position().row + 1
}

/// A node's source, collapsed onto one line and truncated, for quoting back to
/// the user in a finding's message.
pub fn snippet(node: Node<'_>, src: &str, max_chars: usize) -> String {
    let collapsed = text(node, src)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let truncated: String = collapsed.chars().take(max_chars).collect();
    format!("{truncated}…")
}
