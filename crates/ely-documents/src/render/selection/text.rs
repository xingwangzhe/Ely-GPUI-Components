use std::ops::Range;

use crate::tree::{Inline, Node};

#[derive(Default)]
pub(super) struct Text {
    pub words: String,
    pub runs: Vec<Range<usize>>,
    pub links: Vec<(Range<usize>, String)>,
}

impl Text {
    pub(super) fn of(nodes: &[Node]) -> Self {
        let mut text = Self::default();
        text.nodes(nodes);
        text.trim(0);
        text
    }

    fn run(&mut self, text: &str) {
        let start = self.words.len();
        self.words.push_str(text);
        self.runs.push(start..self.words.len());
    }

    fn trim(&mut self, from: usize) {
        let end = self.words.trim_end_matches('\n').len();
        self.words
            .truncate(end.max(from).max(self.runs.last().map_or(0, |run| run.end)));
    }

    fn inline(&mut self, inline: &Inline) {
        let start = self.words.len();
        self.links.extend(
            inline
                .links
                .iter()
                .map(|(range, url)| (start + range.start..start + range.end, url.clone())),
        );
        self.run(&inline.text);
    }

    fn nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Paragraph(inline) | Node::Heading(_, inline) => {
                    self.inline(inline);
                    self.words.push_str("\n\n");
                }
                Node::Code { text, .. } | Node::FrontMatter { text, .. } => {
                    self.run(text);
                    self.words.push_str("\n\n");
                }
                Node::Quote(nodes) => self.nodes(nodes),
                Node::Footnote { label, body } => {
                    self.run(&format!("[{label}]"));
                    self.words.push(' ');
                    self.nodes(body);
                }
                Node::Alert { kind, body } => {
                    self.run(kind.title());
                    self.words.push('\n');
                    self.nodes(body);
                }
                Node::List { start, items } => {
                    for (ix, (checked, nodes)) in items.iter().enumerate() {
                        let marker = match (checked, start) {
                            (Some(true), _) => "[x] ".to_owned(),
                            (Some(false), _) => "[ ] ".to_owned(),
                            (_, Some(start)) => format!("{}. ", start + ix as u64),
                            _ => "• ".to_owned(),
                        };
                        self.run(marker.trim_end());
                        self.words.push(' ');
                        self.nodes(nodes);
                    }
                }
                Node::Table { head, rows, .. } => {
                    for row in std::iter::once(head).chain(rows.iter()) {
                        for (ix, cell) in row.iter().enumerate() {
                            if ix > 0 {
                                self.words.push('\t');
                            }
                            let from = self.words.len();
                            self.nodes(cell);
                            self.trim(from);
                        }
                        self.words.push('\n');
                    }
                    self.words.push('\n');
                }
                Node::Image { alt, .. } => {
                    self.run(alt);
                    self.words.push_str("\n\n");
                }
                Node::Math(tex) => {
                    self.run(tex);
                    self.words.push_str("\n\n");
                }
                Node::Toc(headings) => {
                    for heading in headings {
                        self.run(&heading.title);
                        self.words.push('\n');
                    }
                }
                Node::Rule => self.words.push('\n'),
            }
        }
    }
}
