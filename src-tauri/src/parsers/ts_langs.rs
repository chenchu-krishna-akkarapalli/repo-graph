//! Per-language tree-sitter queries.
//!
//! Each `spec_*` returns the grammar plus one query using the capture-name
//! contract documented in [`super::ts_engine`]. Keeping every query in one
//! file makes it obvious which languages have real AST coverage and how
//! deep that coverage goes.

use super::ts_engine::LanguageSpec;

// ---------------------------------------------------------------- Java -----

const JAVA_QUERY: &str = r#"
(class_declaration
  name: (identifier) @name
  superclass: (superclass [(type_identifier) (generic_type (type_identifier))] @extends)?
  interfaces: (super_interfaces (type_list [(type_identifier) (generic_type (type_identifier))] @implements))?) @sym.class

(interface_declaration name: (identifier) @name) @sym.interface
(enum_declaration name: (identifier) @name) @sym.enum
(record_declaration name: (identifier) @name) @sym.class
(method_declaration name: (identifier) @name) @sym.method
(constructor_declaration name: (identifier) @name) @sym.method

(import_declaration (scoped_identifier) @import)
"#;

pub fn spec_java() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_java::LANGUAGE.into(),
        query_source: JAVA_QUERY,
    }
}

// ------------------------------------------------------------------ C# -----

const CSHARP_QUERY: &str = r#"
(class_declaration name: (identifier) @name) @sym.class
(interface_declaration name: (identifier) @name) @sym.interface
(struct_declaration name: (identifier) @name) @sym.struct
(enum_declaration name: (identifier) @name) @sym.enum
(record_declaration name: (identifier) @name) @sym.class
(method_declaration name: (identifier) @name) @sym.method
(constructor_declaration name: (identifier) @name) @sym.method
(property_declaration name: (identifier) @name) @sym.property

(using_directive (qualified_name) @import)
(using_directive (identifier) @import)
"#;

pub fn spec_csharp() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_c_sharp::LANGUAGE.into(),
        query_source: CSHARP_QUERY,
    }
}

// -------------------------------------------------------------- Kotlin -----

// `tree-sitter-kotlin-ng` names every declared identifier `identifier` (not
// `simple_identifier`/`type_identifier` as the older grammar did) and wraps
// dotted paths in `qualified_identifier`.
const KOTLIN_QUERY: &str = r#"
(class_declaration "class" (identifier) @name
  (delegation_specifiers
    (delegation_specifier
      (constructor_invocation (user_type (identifier) @extends))))?) @sym.class
(class_declaration "interface" (identifier) @name) @sym.interface
(object_declaration (identifier) @name) @sym.object
(function_declaration (identifier) @name) @sym.function

(import (qualified_identifier) @import)
"#;

pub fn spec_kotlin() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_kotlin_ng::LANGUAGE.into(),
        query_source: KOTLIN_QUERY,
    }
}

// --------------------------------------------------------------- Swift -----

// Swift's grammar routes `class`, `struct`, `enum` and `actor` all through
// `class_declaration`, so the keyword itself is matched to recover the kind.
const SWIFT_QUERY: &str = r#"
(class_declaration "struct" (type_identifier) @name) @sym.struct
(class_declaration "enum" (type_identifier) @name) @sym.enum
(class_declaration "actor" (type_identifier) @name) @sym.class
(class_declaration "class" (type_identifier) @name) @sym.class
(protocol_declaration (type_identifier) @name) @sym.interface
(function_declaration (simple_identifier) @name) @sym.function

(import_declaration (identifier) @import)
"#;

pub fn spec_swift() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_swift::LANGUAGE.into(),
        query_source: SWIFT_QUERY,
    }
}

// ------------------------------------------------------------------- C -----

const C_QUERY: &str = r#"
(function_definition
  declarator: (function_declarator declarator: (identifier) @name)) @sym.function
(struct_specifier name: (type_identifier) @name) @sym.struct
(union_specifier name: (type_identifier) @name) @sym.struct
(enum_specifier name: (type_identifier) @name) @sym.enum
(type_definition declarator: (type_identifier) @name) @sym.type_alias

(preproc_include path: (string_literal) @import)
(preproc_include path: (system_lib_string) @import)
"#;

pub fn spec_c() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_c::LANGUAGE.into(),
        query_source: C_QUERY,
    }
}

// ----------------------------------------------------------------- C++ -----

const CPP_QUERY: &str = r#"
(function_definition
  declarator: (function_declarator declarator: (identifier) @name)) @sym.function
(function_definition
  declarator: (function_declarator declarator: (qualified_identifier) @name)) @sym.function
(class_specifier name: (type_identifier) @name) @sym.class
(struct_specifier name: (type_identifier) @name) @sym.struct
(enum_specifier name: (type_identifier) @name) @sym.enum
(namespace_definition name: (namespace_identifier) @name) @sym.namespace

(preproc_include path: (string_literal) @import)
(preproc_include path: (system_lib_string) @import)
"#;

pub fn spec_cpp() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_cpp::LANGUAGE.into(),
        query_source: CPP_QUERY,
    }
}

// ------------------------------------------------------------------ Go -----

const GO_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @sym.function
(method_declaration name: (field_identifier) @name) @sym.method
(type_declaration (type_spec name: (type_identifier) @name type: (struct_type))) @sym.struct
(type_declaration (type_spec name: (type_identifier) @name type: (interface_type))) @sym.interface
(type_declaration (type_spec name: (type_identifier) @name)) @sym.type_alias

(import_spec path: (interpreted_string_literal) @import)
"#;

pub fn spec_go() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_go::LANGUAGE.into(),
        query_source: GO_QUERY,
    }
}

// -------------------------------------------------------------- Python -----

const PYTHON_QUERY: &str = r#"
(class_definition
  name: (identifier) @name
  superclasses: (argument_list (identifier) @extends)?) @sym.class
(function_definition name: (identifier) @name) @sym.function

(import_statement (dotted_name) @import)
(import_from_statement module_name: (dotted_name) @import)
(import_from_statement module_name: (relative_import) @import)
"#;

pub fn spec_python() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_python::LANGUAGE.into(),
        query_source: PYTHON_QUERY,
    }
}

// ---------------------------------------------------------------- Rust -----

const RUST_QUERY: &str = r#"
(function_item name: (identifier) @name) @sym.function
(struct_item name: (type_identifier) @name) @sym.struct
(enum_item name: (type_identifier) @name) @sym.enum
(trait_item name: (type_identifier) @name) @sym.interface
(mod_item name: (identifier) @name) @sym.module
(type_item name: (type_identifier) @name) @sym.type_alias
(const_item name: (identifier) @name) @sym.constant
(static_item name: (identifier) @name) @sym.constant

(use_declaration (scoped_identifier) @import)
(use_declaration (identifier) @import)
(mod_item name: (identifier) @import !body)
"#;

pub fn spec_rust() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_rust::LANGUAGE.into(),
        query_source: RUST_QUERY,
    }
}

// ----------------------------------------------------------------- PHP -----

const PHP_QUERY: &str = r#"
(class_declaration
  name: (name) @name
  (base_clause (name) @extends)?) @sym.class
(interface_declaration name: (name) @name) @sym.interface
(trait_declaration name: (name) @name) @sym.interface
(function_definition name: (name) @name) @sym.function
(method_declaration name: (name) @name) @sym.method

(namespace_use_clause (qualified_name) @import)
"#;

pub fn spec_php() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_php::LANGUAGE_PHP.into(),
        query_source: PHP_QUERY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    /// A query that fails to compile silently disables extraction for that
    /// language — the engine returns `None` and the file yields no symbols.
    /// This catches a grammar upgrade renaming a node before it ships.
    #[test]
    fn every_query_compiles_against_its_grammar() {
        let specs: Vec<(&str, LanguageSpec)> = vec![
            ("java", spec_java()),
            ("csharp", spec_csharp()),
            ("kotlin", spec_kotlin()),
            ("swift", spec_swift()),
            ("c", spec_c()),
            ("cpp", spec_cpp()),
            ("go", spec_go()),
            ("python", spec_python()),
            ("rust", spec_rust()),
            ("php", spec_php()),
        ];
        for (name, spec) in specs {
            if let Err(e) = Query::new(&spec.language, spec.query_source) {
                panic!("{name} query does not compile: {e:?}");
            }
        }
    }
}
