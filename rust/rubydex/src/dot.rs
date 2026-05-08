use std::collections::HashSet;
use std::fmt::Write;

use crate::model::{
    built_in,
    declaration::Declaration,
    definitions::{Definition, Mixin},
    document::Document,
    graph::Graph,
    ids::{DeclarationId, DefinitionId},
};

const DOC_COLOR: &str = "#4a90d9";
const DOC_FILL: &str = "#dce8f5";
const DEF_COLOR: &str = "#e8912d";
const DEF_FILL: &str = "#fdf0e0";
const DECL_COLOR: &str = "#5ba55b";
const DECL_FILL: &str = "#e0f0e0";
const NESTS_COLOR: &str = "#f0c08a";
const MEMBER_COLOR: &str = "#a3d9a3";
const SUPERCLASS_COLOR: &str = "#d94a7a";
const MIXIN_COLOR: &str = "#8b5fc7";


pub struct DotBuilder<'a> {
    output: String,
    graph: &'a Graph,
}

impl<'a> DotBuilder<'a> {
    fn new(graph: &'a Graph) -> Self {
        Self {
            output: String::new(),
            graph,
        }
    }

    fn graph(&self) -> &'a Graph {
        self.graph
    }

    fn writeln(&mut self, s: &str) {
        self.output.push_str(s);
        self.output.push('\n');
    }

    fn html_escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    fn label(type_name: &str, name: &str, color: &str) -> String {
        let escaped = Self::html_escape(name);
        format!(
            concat!(
                "<<table border=\"0\" cellborder=\"0\" cellspacing=\"0\" align=\"center\">",
                "<tr><td align=\"center\"><font point-size=\"8\" color=\"{}\">{}</font></td></tr>",
                "<tr><td align=\"center\"><b>{}</b></td></tr>",
                "</table>>",
            ),
            color, type_name, escaped,
        )
    }

    #[must_use]
    pub fn generate(
        graph: &'a Graph,
        show_builtins: bool,
        show_documents: bool,
        decl_filter: Option<&[String]>,
        def_filter: Option<&[String]>,
        edge_filter: Option<&[String]>,
    ) -> String {
        let mut builder = Self::new(graph);
        let decl_filter: Option<HashSet<&str>> = decl_filter.map(|kinds| {
            if kinds.len() == 1 && kinds[0].eq_ignore_ascii_case("false") {
                HashSet::new()
            } else {
                kinds.iter().map(String::as_str).collect()
            }
        });
        let def_filter: Option<HashSet<&str>> = def_filter.map(|kinds| {
            if kinds.len() == 1 && kinds[0].eq_ignore_ascii_case("false") {
                HashSet::new()
            } else {
                kinds.iter().map(String::as_str).collect()
            }
        });
        let edge_filter: Option<HashSet<&str>> = edge_filter.map(|kinds| {
            if kinds.len() == 1 && kinds[0].eq_ignore_ascii_case("false") {
                HashSet::new()
            } else {
                kinds.iter().map(String::as_str).collect()
            }
        });
        let show_edge = |label: &str| -> bool {
            match &edge_filter {
                Some(filter) => filter.contains(label),
                None => true,
            }
        };

        builder.writeln("digraph rubydex {");
        builder.writeln("  rankdir=TB");
        builder.writeln("  node [fontname=\"Courier\" fontsize=10 shape=box]");
        builder.writeln("  edge [fontsize=9 fontname=\"Courier\"]");
        builder.output.push('\n');

        // 1. Collect documents, write nodes, collect all definition IDs
        let mut documents: Vec<_> = graph.documents().values()
            .filter(|d| show_builtins || d.uri() != built_in::BUILT_IN_URI)
            .collect();
        documents.sort_by(|a, b| a.uri().cmp(b.uri()));

        let mut all_def_ids: HashSet<&DefinitionId> = HashSet::new();
        for document in &documents {
            if show_documents {
                document.to_dot(&mut builder);
            }
            for def_id in document.definitions() {
                all_def_ids.insert(def_id);
            }
        }
        builder.output.push('\n');

        // 2. Collect definitions from documents, write visible ones, collect declaration IDs from all
        let mut definitions: Vec<_> = graph
            .definitions()
            .iter()
            .filter(|(id, _)| all_def_ids.contains(id))
            .filter_map(|(_, definition)| {
                let decl_id = graph.definition_to_declaration_id(definition)?;
                let declaration = graph.declarations().get(decl_id)?;
                let sort_key = format!("{}({})", definition.kind(), declaration.name());
                Some((sort_key, definition))
            })
            .collect();
        definitions.sort_by(|a, b| a.0.cmp(&b.0));

        let mut decl_ids: HashSet<&DeclarationId> = HashSet::new();
        for (_, definition) in &definitions {
            let visible = match def_filter {
                Some(ref filter) => filter.contains(definition.kind()),
                None => true,
            };
            if visible {
                definition.to_dot(&mut builder);
            } else {
                all_def_ids.remove(&definition.id());
            }
            if let Some(decl_id) = graph.definition_to_declaration_id(definition) {
                decl_ids.insert(decl_id);
            }
        }
        builder.output.push('\n');

        // 3. Collect declarations from definitions, write nodes (apply decl filter)
        let mut declarations: Vec<_> = graph.declarations().iter()
            .filter(|(id, decl)| {
                let reachable = decl_ids.contains(id);
                match decl_filter {
                    Some(ref filter) => reachable && filter.contains(decl.kind()),
                    None => reachable,
                }
            })
            .collect();
        declarations.sort_by(|a, b| a.1.name().cmp(b.1.name()));
        if decl_filter.is_some() {
            decl_ids.clear();
            for (id, _) in &declarations {
                decl_ids.insert(id);
            }
        }
        for (_, declaration) in &declarations {
            declaration.to_dot(&mut builder);
        }
        builder.output.push('\n');

        // Document -> Definition edges (defines)
        if show_documents && show_edge("defines") {
            for document in &documents {
                let uri = document.uri();
                let doc_id = Self::doc_node_id(uri);
                for def_id in document.definitions() {
                    if all_def_ids.contains(def_id) {
                        let _ = writeln!(
                            builder.output,
                            "  {doc_id} -> \"def_{def_id}\" [label=\"defines\" color=\"{DEF_COLOR}\" fontcolor=\"{DEF_COLOR}\"]"
                        );
                    }
                }
            }
        }
        builder.output.push('\n');

        // Definition -> Declaration edges (declares)
        if show_edge("declares") {
            for (_, definition) in &definitions {
                if !all_def_ids.contains(&definition.id()) {
                    continue;
                }
                let def_id = definition.id();
                if let Some(decl_id) = graph.definition_to_declaration_id(definition) {
                    if !decl_ids.contains(decl_id) {
                        continue;
                    }
                    if let Some(declaration) = graph.declarations().get(decl_id) {
                        let decl_node = Self::decl_node_id(declaration.name());
                        let _ = writeln!(
                            builder.output,
                            "  \"def_{def_id}\" -> {decl_node} [label=\"declares\" color=\"{DECL_COLOR}\" fontcolor=\"{DECL_COLOR}\"]"
                        );
                    }
                }
            }
        }
        builder.output.push('\n');

        // Definition -> Definition edges (nesting)
        if show_edge("contains") {
            for (_, definition) in &definitions {
                if !all_def_ids.contains(&definition.id()) {
                    continue;
                }
                let parent_id = definition.id();
                let children: &[DefinitionId] = match definition {
                    Definition::Class(d) => d.members(),
                    Definition::Module(d) => d.members(),
                    Definition::SingletonClass(d) => d.members(),
                    _ => &[],
                };
                for child_id in children {
                    if all_def_ids.contains(child_id) {
                        let _ = writeln!(
                            builder.output,
                            "  \"def_{parent_id}\" -> \"def_{child_id}\" [label=\"contains\" style=dashed arrowhead=onormal color=\"{NESTS_COLOR}\" fontcolor=\"{NESTS_COLOR}\"]"
                        );
                    }
                }
            }
        }
        builder.output.push('\n');

        // Declaration -> Declaration edges (superclass)
        if show_edge("inherits") {
            for (_, definition) in &definitions {
                if let Definition::Class(class_def) = definition {
                    if let Some(superclass_ref_id) = class_def.superclass_ref() {
                        if let Some(decl_id) = builder.resolve_ref(superclass_ref_id) {
                            if !decl_ids.contains(decl_id) {
                                continue;
                            }
                            if let Some(declaration) = graph.declarations().get(decl_id) {
                                if let Some(child_decl_id) = graph.definition_to_declaration_id(definition) {
                                    if let Some(child_decl) = graph.declarations().get(child_decl_id) {
                                        let child_node = Self::decl_node_id(child_decl.name());
                                        let parent_node = Self::decl_node_id(declaration.name());
                                        let _ = writeln!(
                                            builder.output,
                                            "  {parent_node} -> {child_node} [label=\"inherits\" dir=back color=\"{SUPERCLASS_COLOR}\" fontcolor=\"{SUPERCLASS_COLOR}\"]"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        builder.output.push('\n');

        // Declaration -> Declaration edges (mixins: include/prepend/extend)
        for (_, definition) in &definitions {
            let mixins: &[Mixin] = match definition {
                Definition::Class(d) => d.mixins(),
                Definition::Module(d) => d.mixins(),
                Definition::SingletonClass(d) => d.mixins(),
                _ => &[],
            };
            if mixins.is_empty() {
                continue;
            }
            let Some(decl_id) = graph.definition_to_declaration_id(definition) else {
                continue;
            };
            let Some(decl) = graph.declarations().get(decl_id) else {
                continue;
            };
            let src_node = Self::decl_node_id(decl.name());
            for mixin in mixins {
                let mixin_label = match mixin {
                    Mixin::Include(_) => "includes",
                    Mixin::Prepend(_) => "prepends",
                    Mixin::Extend(_) => "extends",
                };
                if !show_edge(mixin_label) {
                    continue;
                }
                if let Some(target_decl_id) = builder.resolve_ref(mixin.constant_reference_id()) {
                    if !decl_ids.contains(target_decl_id) {
                        continue;
                    }
                    if let Some(target_decl) = graph.declarations().get(target_decl_id) {
                        let target_node = Self::decl_node_id(target_decl.name());
                        let _ = writeln!(
                            builder.output,
                            "  {target_node} -> {src_node} [label=\"{mixin_label}\" dir=back color=\"{MIXIN_COLOR}\" fontcolor=\"{MIXIN_COLOR}\"]"
                        );
                    }
                }
            }
        }
        builder.output.push('\n');

        // Declaration -> Declaration edges (member)
        if show_edge("owns") {
            for (_, declaration) in &declarations {
                if let Some(namespace) = declaration.as_namespace() {
                    let owner_node = Self::decl_node_id(declaration.name());
                    let mut members: Vec<_> = namespace.members().values()
                        .filter(|id| decl_ids.contains(id))
                        .collect();
                    members.sort();
                    for member_id in members {
                        if let Some(member) = graph.declarations().get(member_id) {
                            let member_node = Self::decl_node_id(member.name());
                            let _ = writeln!(
                                builder.output,
                                "  {owner_node} -> {member_node} [label=\"owns\" style=dashed arrowhead=onormal color=\"{MEMBER_COLOR}\" fontcolor=\"{MEMBER_COLOR}\"]"
                            );
                        }
                    }
                }
            }
        }

        builder.writeln("}");
        builder.output
    }

    fn resolve_ref(&self, ref_id: &crate::model::ids::ConstantReferenceId) -> Option<&'a DeclarationId> {
        let constant_ref = self.graph.constant_references().get(ref_id)?;
        self.graph.name_id_to_declaration_id(*constant_ref.name_id())
    }

    fn doc_node_id(uri: &str) -> String {
        format!("\"doc_{}\"", uri.replace(|c: char| !c.is_alphanumeric(), "_"))
    }

    fn decl_node_id(name: &str) -> String {
        format!("\"decl_{}\"", name.replace(|c: char| !c.is_alphanumeric(), "_"))
    }
}

pub trait ToDot {
    fn to_dot(&self, builder: &mut DotBuilder);
}

impl ToDot for Document {
    fn to_dot(&self, builder: &mut DotBuilder) {
        let uri = self.uri();
        let label = uri.rsplit('/').next().unwrap_or(uri);
        let node_id = DotBuilder::doc_node_id(uri);
        let html_label = DotBuilder::label("Document", label, DOC_COLOR);
        let _ = writeln!(
            builder.output,
            "  {node_id} [label={html_label} shape=note color=\"{DOC_COLOR}\" fillcolor=\"{DOC_FILL}\" style=filled]"
        );
    }
}

impl ToDot for Definition {
    fn to_dot(&self, builder: &mut DotBuilder) {
        let def_id = self.id();
        let Some(decl_id) = builder.graph().definition_to_declaration_id(self) else {
            return;
        };
        let Some(declaration) = builder.graph().declarations().get(decl_id) else {
            return;
        };

        let type_label = format!("{}Def", self.kind());
        let html_label = DotBuilder::label(&type_label, declaration.name(), DEF_COLOR);
        let _ = writeln!(
            builder.output,
            "  \"def_{def_id}\" [label={html_label} style=rounded color=\"{DEF_COLOR}\" fillcolor=\"{DEF_FILL}\" style=\"rounded,filled\"]"
        );
    }
}

impl ToDot for Declaration {
    fn to_dot(&self, builder: &mut DotBuilder) {
        let type_label = format!("{}Decl", self.kind());
        let node_id = DotBuilder::decl_node_id(self.name());
        let html_label = DotBuilder::label(&type_label, self.name(), DECL_COLOR);
        let _ = writeln!(
            builder.output,
            "  {node_id} [label={html_label} color=\"{DECL_COLOR}\" fillcolor=\"{DECL_FILL}\" style=filled]"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::GraphTest;

    #[test]
    fn test_dot_generation() {
        let mut context = GraphTest::new();
        context.index_uri(
            "file:///test.rb",
            "
                class TestClass
                end

                module TestModule
                end
            ",
        );
        context.resolve();
        let dot_output = DotBuilder::generate(context.graph(), true, true, None, None, None);

        assert!(dot_output.contains("digraph rubydex"));

        // Document nodes
        assert!(dot_output.contains("Document"));
        assert!(dot_output.contains("test.rb"));

        // Definition nodes
        assert!(dot_output.contains("ClassDef"));
        assert!(dot_output.contains("ModuleDef"));

        // Declaration nodes
        assert!(dot_output.contains("ClassDecl"));
        assert!(dot_output.contains("ModuleDecl"));

        // Edges
        assert!(dot_output.contains("defines"));
        assert!(dot_output.contains("declares"));
        assert!(dot_output.contains("owns"));
    }

    #[test]
    fn test_dot_nesting_edges() {
        let mut context = GraphTest::new();
        context.index_uri(
            "file:///test.rb",
            "
                module Outer
                  class Inner
                  end
                end
            ",
        );
        context.resolve();
        let dot_output = DotBuilder::generate(context.graph(), false, true, None, None, None);
        assert!(dot_output.contains("contains"));
    }

    #[test]
    fn test_dot_superclass_edges() {
        let mut context = GraphTest::new();
        context.index_uri(
            "file:///test.rb",
            "
                class Parent
                end

                class Child < Parent
                end
            ",
        );
        context.resolve();
        let dot_output = DotBuilder::generate(context.graph(), false, true, None, None, None);
        assert!(dot_output.contains("inherits"));
    }

    #[test]
    fn test_dot_mixin_edges() {
        let mut context = GraphTest::new();
        context.index_uri(
            "file:///test.rb",
            "
                module Mixin
                end

                class Klass
                  include Mixin
                end
            ",
        );
        context.resolve();
        let dot_output = DotBuilder::generate(context.graph(), false, true, None, None, None);
        assert!(dot_output.contains("includes"));
    }

    #[test]
    fn test_dot_reopened_builtin_not_hidden() {
        let mut context = GraphTest::new();
        context.index_uri(
            "file:///test.rb",
            "
                class Object
                  def test; end
                end
            ",
        );
        context.resolve();
        let dot_output = DotBuilder::generate(context.graph(), false, true, None, None, None);

        assert!(dot_output.contains("ClassDecl"));
        assert!(dot_output.contains("Object"));
    }

    #[test]
    fn test_dot_filter_declarations() {
        let mut context = GraphTest::new();
        context.index_uri(
            "file:///test.rb",
            "
                class TestClass
                  def test_method; end
                end

                module TestModule
                end
            ",
        );
        context.resolve();

        let filter = vec!["Class".to_string()];
        let dot_output = DotBuilder::generate(context.graph(), false, true, Some(&filter), None, None);

        assert!(dot_output.contains("ClassDecl"));
        assert!(!dot_output.contains("ModuleDecl"));
        assert!(!dot_output.contains("MethodDecl"));
        // Definitions are still shown
        assert!(dot_output.contains("ModuleDef"));
        assert!(dot_output.contains("MethodDef"));
    }

    #[test]
    fn test_dot_filter_definitions() {
        let mut context = GraphTest::new();
        context.index_uri(
            "file:///test.rb",
            "
                class TestClass
                  def test_method; end
                end

                module TestModule
                end
            ",
        );
        context.resolve();

        let filter = vec!["Class".to_string()];
        let dot_output = DotBuilder::generate(context.graph(), false, true, None, Some(&filter), None);

        assert!(dot_output.contains("ClassDef"));
        assert!(!dot_output.contains("ModuleDef"));
        assert!(!dot_output.contains("MethodDef"));
        // Declarations are still shown for reachable definitions
        assert!(dot_output.contains("ClassDecl"));
    }
}
