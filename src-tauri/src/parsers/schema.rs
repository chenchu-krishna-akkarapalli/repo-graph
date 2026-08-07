//! ORM models, GraphQL/tRPC procedures, and event/pub-sub channels.
//!
//! Same shape as `state.rs`: a shared line scanner every extractor can call,
//! AST-based or not. Every rule is **gated on the framework actually being
//! referenced in the file** — factory names like `model`, `topic` and `Entity`
//! are far too generic to match blind, and an ungated scanner reports garbage
//! (the mistake `state.rs` was fixed for: a `//` comment and a test string
//! literal both matched a bare `mpsc::channel`).
//!
//! Kinds emitted:
//! - `database_schema` — a persisted model/table declaration
//! - `route` — a GraphQL or tRPC procedure (same class as an HTTP endpoint)
//! - `event_channel` — a named pub/sub topic, subject, or emitter event

use super::state::{binding_for, ident_char};

pub const DATABASE_SCHEMA: &str = "database_schema";
pub const ROUTE: &str = "route";
pub const EVENT_CHANNEL: &str = "event_channel";

/// A detected schema/route/channel declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Detected {
    pub name: String,
    pub line: usize,
    pub kind: &'static str,
}

fn push_unique(out: &mut Vec<Detected>, name: String, line: usize, kind: &'static str) {
    if name.is_empty() || name.len() > 128 {
        return;
    }
    if !out.iter().any(|d| d.name == name && d.kind == kind) {
        out.push(Detected { name, line, kind });
    }
}

/// Identifier that follows `keyword ` on a line (`model User {` → `User`).
fn name_after_keyword(line: &str, keyword: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix(keyword)?;
    let rest = rest.trim_start();
    let name: String = rest.chars().take_while(|c| ident_char(*c)).collect();
    (!name.is_empty()).then_some(name)
}

/// First single- or double-quoted string on the line (`emit('user:created')`).
fn first_string_literal(line: &str) -> Option<String> {
    let bytes: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '\'' || c == '"' || c == '`' {
            let close = bytes.iter().skip(i + 1).position(|&x| x == c)? + i + 1;
            let literal: String = bytes[i + 1..close].iter().collect();
            return (!literal.is_empty()).then_some(literal);
        }
        i += 1;
    }
    None
}

/// The string argument of `call(` on this line — `emit("x")` → `x`.
fn string_arg_of(line: &str, call: &str) -> Option<String> {
    let at = line.find(call)?;
    let after = &line[at + call.len()..];
    let after = after.trim_start();
    if !after.starts_with('(') {
        return None;
    }
    first_string_literal(after)
}

/// True when a comment or an obvious non-declaration should be skipped.
fn is_skippable(line: &str) -> bool {
    let t = line.trim_start();
    // `#[...]` is a Rust attribute, not a comment — skipping it would drop
    // every `#[derive(Queryable)]` marker this module keys off.
    t.is_empty()
        || t.starts_with("//")
        || (t.starts_with('#') && !t.starts_with("#["))
        || t.starts_with("/*")
        || t.starts_with('*')
        || t.starts_with("--")
}

/// JS/TS: Drizzle, TypeORM, Sequelize, Mongoose, Prisma client; tRPC and
/// GraphQL SDL in template literals; EventEmitter / RxJS / Redis / Kafka.
pub fn scan_js(source: &str) -> Vec<Detected> {
    let mentions = |needle: &str| source.contains(needle);

    let drizzle = mentions("drizzle-orm");
    let typeorm = mentions("typeorm");
    let sequelize = mentions("sequelize");
    let mongoose = mentions("mongoose");
    let trpc = mentions("@trpc") || mentions("trpc");
    let graphql = mentions("graphql") || mentions("apollo");
    let rxjs = mentions("rxjs");
    let redis = mentions("redis") || mentions("ioredis");
    let kafka = mentions("kafkajs") || mentions("kafka");
    let emitter = mentions("EventEmitter") || mentions("events") || mentions("socket.io");

    // `@Entity()` / `@Schema()` decorate the class on a following line.
    let mut pending_entity: Option<usize> = None;
    let mut out = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (idx, raw) in lines.iter().enumerate() {
        let line_no = idx + 1;
        if is_skippable(raw) {
            continue;
        }
        let line = raw.trim();

        if typeorm && (line.starts_with("@Entity") || line.starts_with("@ViewEntity")) {
            pending_entity = Some(line_no);
            continue;
        }
        if let Some(decorator_line) = pending_entity {
            if let Some(name) = name_after_keyword(line, "export class ")
                .or_else(|| name_after_keyword(line, "class "))
            {
                push_unique(&mut out, name, decorator_line, DATABASE_SCHEMA);
                pending_entity = None;
                continue;
            }
            // Only the immediately following declaration counts.
            if !line.starts_with('@') {
                pending_entity = None;
            }
        }

        if drizzle {
            for factory in ["pgTable", "mysqlTable", "sqliteTable"] {
                if let Some(name) = binding_for(raw, factory) {
                    push_unique(&mut out, name, line_no, DATABASE_SCHEMA);
                }
            }
        }
        if sequelize {
            if let Some(name) = string_arg_of(raw, ".define") {
                push_unique(&mut out, name, line_no, DATABASE_SCHEMA);
            }
        }
        if mongoose {
            for factory in ["mongoose.model", "model"] {
                if let Some(name) = string_arg_of(raw, factory) {
                    push_unique(&mut out, name, line_no, DATABASE_SCHEMA);
                    break;
                }
            }
            if let Some(name) = binding_for(raw, "Schema") {
                push_unique(&mut out, name, line_no, DATABASE_SCHEMA);
            }
        }

        // tRPC: `getUser: publicProcedure.query(...)` — the property name is
        // the procedure, so read the binding to the left of the colon.
        if trpc && (line.contains("Procedure.query") || line.contains("Procedure.mutation") || line.contains("Procedure.subscription")) {
            if let Some((lhs, _)) = line.split_once(':') {
                let name: String = lhs.trim().chars().take_while(|c| ident_char(*c)).collect();
                push_unique(&mut out, name, line_no, ROUTE);
            }
        }

        // GraphQL SDL, usually inside a gql`...` template literal.
        if graphql {
            if let Some(t) = name_after_keyword(line, "type ") {
                if matches!(t.as_str(), "Query" | "Mutation" | "Subscription") {
                    // Field names of the operation block follow until `}`.
                    for probe in lines.iter().skip(idx + 1) {
                        let p = probe.trim();
                        if p.starts_with('}') || p.is_empty() {
                            break;
                        }
                        let field: String = p.chars().take_while(|c| ident_char(*c)).collect();
                        if !field.is_empty() {
                            push_unique(&mut out, format!("{t}.{field}"), line_no, ROUTE);
                        }
                    }
                }
            }
        }

        if rxjs {
            for factory in ["Subject", "BehaviorSubject", "ReplaySubject", "EventEmitter"] {
                if let Some(name) = binding_for(raw, factory) {
                    push_unique(&mut out, name, line_no, EVENT_CHANNEL);
                }
            }
        }
        if emitter {
            for call in [".emit", ".on", ".once", ".addListener"] {
                if let Some(topic) = string_arg_of(raw, call) {
                    push_unique(&mut out, topic, line_no, EVENT_CHANNEL);
                }
            }
        }
        if redis {
            for call in [".publish", ".subscribe", ".psubscribe"] {
                if let Some(topic) = string_arg_of(raw, call) {
                    push_unique(&mut out, topic, line_no, EVENT_CHANNEL);
                }
            }
        }
        if kafka && (line.contains("topic") || line.contains("Topic")) {
            if let Some(topic) = first_string_literal(raw) {
                push_unique(&mut out, topic, line_no, EVENT_CHANNEL);
            }
        }
    }
    out
}

/// Python: SQLAlchemy declarative models, Django models, Redis/Kafka pub-sub.
pub fn scan_python(source: &str) -> Vec<Detected> {
    let mentions = |needle: &str| source.contains(needle);
    let sqlalchemy = mentions("sqlalchemy") || mentions("SQLModel");
    let django = mentions("django") || mentions("models.Model");
    let redis = mentions("redis");
    let kafka = mentions("kafka");

    let mut out = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    // Same guard as `scan_rust`: docstrings and fixture blocks are not code.
    let mut in_triple_quote = false;

    for (idx, raw) in lines.iter().enumerate() {
        let line_no = idx + 1;
        let triples = raw.matches("\"\"\"").count() + raw.matches("'''").count();
        if in_triple_quote {
            if triples % 2 == 1 {
                in_triple_quote = false;
            }
            continue;
        }
        if triples % 2 == 1 {
            in_triple_quote = true;
            continue;
        }
        if is_skippable(raw) {
            continue;
        }
        let line = raw.trim();

        if (sqlalchemy || django) && line.starts_with("class ") {
            let after = &line["class ".len()..];
            let name: String = after.chars().take_while(|c| ident_char(*c)).collect();
            let bases: String = after.chars().skip_while(|c| ident_char(*c)).collect();
            let is_model = bases.contains("Base")
                || bases.contains("models.Model")
                || bases.contains("SQLModel")
                || bases.contains("DeclarativeBase");
            // `__tablename__` inside the body is the other reliable marker.
            let has_tablename = lines
                .iter()
                .skip(idx + 1)
                .take(12)
                .any(|p| p.trim_start().starts_with("__tablename__"));
            if is_model || has_tablename {
                push_unique(&mut out, name, line_no, DATABASE_SCHEMA);
            }
        }

        if redis {
            for call in [".publish", ".subscribe", ".psubscribe"] {
                if let Some(topic) = string_arg_of(raw, call) {
                    push_unique(&mut out, topic, line_no, EVENT_CHANNEL);
                }
            }
        }
        if kafka && (line.contains("topic") || line.contains("Topic")) {
            if let Some(topic) = first_string_literal(raw) {
                push_unique(&mut out, topic, line_no, EVENT_CHANNEL);
            }
        }
    }
    out
}

/// Rust: Diesel `table!` blocks and derive-based ORM models.
pub fn scan_rust(source: &str) -> Vec<Detected> {
    let mentions = |needle: &str| source.contains(needle);
    let diesel = mentions("diesel");
    let sea_orm = mentions("sea_orm");
    let sqlx = mentions("sqlx");

    let mut out = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut pending_derive: Option<usize> = None;
    // Raw string blocks hold sample code — a parser's own test fixtures embed
    // `table! { posts ... }` verbatim. Claiming those is the exact false
    // positive `state.rs` was fixed for; here it would tag this very file.
    let mut in_raw_string = false;

    for (idx, raw) in lines.iter().enumerate() {
        let line_no = idx + 1;
        if in_raw_string {
            if raw.contains("\"#") {
                in_raw_string = false;
            }
            continue;
        }
        if raw.contains("r#\"") && !raw[raw.find("r#\"").unwrap() + 3..].contains("\"#") {
            in_raw_string = true;
            continue;
        }
        if is_skippable(raw) {
            continue;
        }
        let line = raw.trim();

        if diesel && line.starts_with("table!") {
            // `table! { users (id) { .. } }` — the table name is the next ident.
            for probe in lines.iter().skip(idx).take(4) {
                if let Some(name) = probe
                    .trim()
                    .strip_prefix("table!")
                    .map(|r| r.trim_start_matches(['{', ' ']).to_string())
                    .filter(|r| !r.is_empty())
                    .map(|r| r.chars().take_while(|c| ident_char(*c)).collect::<String>())
                    .filter(|n| !n.is_empty())
                {
                    push_unique(&mut out, name, line_no, DATABASE_SCHEMA);
                    break;
                }
                let name: String = probe.trim().chars().take_while(|c| ident_char(*c)).collect();
                if !name.is_empty() && name != "table" {
                    push_unique(&mut out, name, line_no, DATABASE_SCHEMA);
                    break;
                }
            }
            continue;
        }

        let orm_derive = line.starts_with("#[derive")
            && ((diesel && (line.contains("Queryable") || line.contains("Insertable")))
                || (sea_orm && line.contains("DeriveEntityModel"))
                || (sqlx && line.contains("FromRow")));
        if orm_derive {
            pending_derive = Some(line_no);
            continue;
        }
        if let Some(derive_line) = pending_derive {
            let decl = name_after_keyword(line, "pub struct ")
                .or_else(|| name_after_keyword(line, "struct "));
            if let Some(name) = decl {
                push_unique(&mut out, name, derive_line, DATABASE_SCHEMA);
                pending_derive = None;
            } else if !line.starts_with('#') {
                pending_derive = None;
            }
        }
    }
    out
}

/// Go: GORM models (embedded `gorm.Model` or `gorm:"..."` field tags).
pub fn scan_go(source: &str) -> Vec<Detected> {
    if !source.contains("gorm") {
        return Vec::new();
    }
    let mut out = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (idx, raw) in lines.iter().enumerate() {
        if is_skippable(raw) {
            continue;
        }
        let line = raw.trim();
        if !line.starts_with("type ") || !line.contains("struct") {
            continue;
        }
        let name: String = line["type ".len()..]
            .chars()
            .take_while(|c| ident_char(*c))
            .collect();
        // Scan the struct body for a GORM marker before claiming it.
        let mut is_model = false;
        for probe in lines.iter().skip(idx + 1) {
            let p = probe.trim();
            if p.starts_with('}') {
                break;
            }
            if p.contains("gorm.Model") || p.contains("gorm:\"") {
                is_model = true;
                break;
            }
        }
        if is_model {
            push_unique(&mut out, name, idx + 1, DATABASE_SCHEMA);
        }
    }
    out
}

/// Java (JPA `@Entity`), C# (EF `DbSet<T>`), PHP (Doctrine / Eloquent).
pub fn scan_jvm_dotnet_php(source: &str) -> Vec<Detected> {
    let mentions = |needle: &str| source.contains(needle);
    let jpa = mentions("javax.persistence") || mentions("jakarta.persistence");
    let ef = mentions("EntityFrameworkCore") || mentions("DbContext");
    let doctrine = mentions("Doctrine");
    let eloquent = mentions("Illuminate\\Database") || mentions("Eloquent");

    let mut out = Vec::new();
    let mut pending_entity: Option<usize> = None;

    for (idx, raw) in source.lines().enumerate() {
        let line_no = idx + 1;
        if is_skippable(raw) {
            continue;
        }
        let line = raw.trim();

        if (jpa || doctrine) && (line.starts_with("@Entity") || line.starts_with("@ORM\\Entity")) {
            pending_entity = Some(line_no);
            continue;
        }
        if let Some(decorator_line) = pending_entity {
            let decl = ["public class ", "class ", "public final class "]
                .iter()
                .find_map(|kw| name_after_keyword(line, kw));
            if let Some(name) = decl {
                push_unique(&mut out, name, decorator_line, DATABASE_SCHEMA);
                pending_entity = None;
                continue;
            }
            if !line.starts_with('@') {
                pending_entity = None;
            }
        }

        // `public DbSet<User> Users { get; set; }`
        if ef && line.contains("DbSet<") {
            if let Some(start) = line.find("DbSet<") {
                let inner: String = line[start + "DbSet<".len()..]
                    .chars()
                    .take_while(|c| ident_char(*c))
                    .collect();
                push_unique(&mut out, inner, line_no, DATABASE_SCHEMA);
            }
        }

        // `class User extends Model` (Eloquent)
        if eloquent && line.contains(" extends Model") {
            if let Some(name) = name_after_keyword(line, "class ") {
                push_unique(&mut out, name, line_no, DATABASE_SCHEMA);
            }
        }
    }
    out
}

/// `.graphql` / `.gql` SDL files: every field of Query/Mutation/Subscription.
pub fn scan_graphql_sdl(source: &str) -> Vec<Detected> {
    let mut out = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut current: Option<String> = None;

    for (idx, raw) in lines.iter().enumerate() {
        if is_skippable(raw) {
            continue;
        }
        let line = raw.trim();
        if let Some(t) = name_after_keyword(line, "type ") {
            current = matches!(t.as_str(), "Query" | "Mutation" | "Subscription").then_some(t);
            continue;
        }
        if line.starts_with('}') {
            current = None;
            continue;
        }
        if let Some(ref op) = current {
            let field: String = line.chars().take_while(|c| ident_char(*c)).collect();
            if !field.is_empty() {
                push_unique(&mut out, format!("{op}.{field}"), idx + 1, ROUTE);
            }
        }
    }
    out
}

/// `.graphql` / `.gql` schema files.
pub struct GraphqlExtractor;

impl super::Extractor for GraphqlExtractor {
    fn extract(&self, _file_path: &std::path::Path, source: &str) -> super::ExtractionResult {
        let mut result = super::ExtractionResult::default();
        let found = scan_graphql_sdl(source);
        for d in &found {
            result.entry_points.push(super::EntryPoint {
                route: d.name.clone(),
                handler: None,
                line: d.line,
            });
        }
        apply(&found, &mut result.symbols);
        result
    }
}

/// Merge detections into an extractor's symbol list. Existing symbols are
/// re-tagged in place so real line ranges survive (same contract as
/// `state::apply`); anything new is appended as a single-line symbol.
pub fn apply(found: &[Detected], symbols: &mut Vec<super::ExtractedSymbol>) {
    for d in found {
        if let Some(existing) = symbols.iter_mut().find(|s| s.name == d.name) {
            existing.kind = d.kind.to_string();
        } else {
            symbols.push(super::ExtractedSymbol {
                name: d.name.clone(),
                kind: d.kind.to_string(),
                start_line: d.line.max(1),
                end_line: d.line.max(1),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of_kind(found: &[Detected], kind: &str) -> Vec<String> {
        found
            .iter()
            .filter(|d| d.kind == kind)
            .map(|d| d.name.clone())
            .collect()
    }

    fn has(names: &[String], want: &str) -> bool {
        names.iter().any(|n| n == want)
    }

    #[test]
    fn drizzle_and_typeorm_models_are_detected() {
        let src = r#"
import { pgTable, text } from 'drizzle-orm/pg-core'
import { Entity, Column } from 'typeorm'

export const users = pgTable('users', { id: text('id') })

@Entity()
export class Invoice {
  @Column() total: number
}
"#;
        let found = scan_js(src);
        let models = of_kind(&found, DATABASE_SCHEMA);
        assert!(has(&models, "users"), "got {models:?}");
        assert!(has(&models, "Invoice"), "got {models:?}");
    }

    #[test]
    fn trpc_procedures_and_graphql_fields_are_routes() {
        let src = r#"
import { publicProcedure, router } from '@trpc/server'
import { gql } from 'graphql-tag'

export const appRouter = router({
  getUser: publicProcedure.query(({ input }) => db.user(input)),
  createUser: publicProcedure.mutation(({ input }) => db.create(input)),
})

const typeDefs = gql`
  type Query {
    users: [User!]!
  }
`
"#;
        let routes = of_kind(&scan_js(src), ROUTE);
        assert!(has(&routes, "getUser"), "got {routes:?}");
        assert!(has(&routes, "createUser"), "got {routes:?}");
        assert!(has(&routes, "Query.users"), "got {routes:?}");
    }

    #[test]
    fn event_channels_are_detected_from_emitters_and_redis() {
        let src = r#"
import { EventEmitter } from 'events'
import Redis from 'ioredis'

const bus = new EventEmitter()
bus.emit('user:created', payload)
redis.subscribe('cache:invalidate')
"#;
        let events = of_kind(&scan_js(src), EVENT_CHANNEL);
        assert!(has(&events, "user:created"), "got {events:?}");
        assert!(has(&events, "cache:invalidate"), "got {events:?}");
    }

    #[test]
    fn ungated_files_claim_nothing() {
        // No ORM/event library imported anywhere: `model()`, `Entity` and
        // `.emit()` here belong to unrelated application code.
        let src = r#"
const widget = model('gear')
class Entity { }
particle.emit('spark')
export const users = pgTable('users', {})
"#;
        assert_eq!(scan_js(src), Vec::new());
    }

    #[test]
    fn sqlalchemy_and_django_models_are_detected() {
        let src = r#"
from sqlalchemy.orm import DeclarativeBase

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "users"
"#;
        let models = of_kind(&scan_python(src), DATABASE_SCHEMA);
        assert!(has(&models, "User"), "got {models:?}");
    }

    #[test]
    fn gorm_structs_are_detected_only_with_markers() {
        let src = r#"
package main

import "gorm.io/gorm"

type User struct {
    gorm.Model
    Name string
}

type Config struct {
    Port int
}
"#;
        let models = of_kind(&scan_go(src), DATABASE_SCHEMA);
        assert_eq!(models, vec!["User".to_string()], "Config has no gorm marker");
    }

    #[test]
    fn diesel_derives_and_tables_are_detected() {
        let src = r#"
use diesel::prelude::*;

table! {
    posts (id) {
        id -> Integer,
    }
}

#[derive(Queryable, Debug)]
pub struct Post {
    pub id: i32,
}
"#;
        let models = of_kind(&scan_rust(src), DATABASE_SCHEMA);
        assert!(has(&models, "posts"), "got {models:?}");
        assert!(has(&models, "Post"), "got {models:?}");
    }

    #[test]
    fn jpa_ef_and_eloquent_entities_are_detected() {
        let java = "import javax.persistence.Entity;\n@Entity\npublic class Customer { }";
        assert_eq!(of_kind(&scan_jvm_dotnet_php(java), DATABASE_SCHEMA), vec!["Customer".to_string()]);

        let cs = "using Microsoft.EntityFrameworkCore;\npublic class Ctx : DbContext {\n public DbSet<Order> Orders { get; set; }\n}";
        assert_eq!(of_kind(&scan_jvm_dotnet_php(cs), DATABASE_SCHEMA), vec!["Order".to_string()]);

        let php = "<?php\nuse Illuminate\\Database\\Eloquent\\Model;\nclass Product extends Model { }";
        assert_eq!(of_kind(&scan_jvm_dotnet_php(php), DATABASE_SCHEMA), vec!["Product".to_string()]);
    }

    #[test]
    fn graphql_sdl_fields_become_routes() {
        let src = "type Query {\n  users: [User!]!\n  user(id: ID!): User\n}\n\ntype User {\n  id: ID!\n}";
        let routes = of_kind(&scan_graphql_sdl(src), ROUTE);
        assert_eq!(routes, vec!["Query.users".to_string(), "Query.user".to_string()], "User type is not an operation");
    }

    /// Regression: the first run of this scanner tagged `posts` and `Post` in
    /// `schema.rs` itself, reading its own raw-string test fixtures as code.
    #[test]
    fn rust_raw_string_fixtures_are_not_claimed_as_models() {
        let src = "use diesel::prelude::*;\n\n#[test]\nfn t() {\n    let src = r#\"\ntable! {\n    posts (id) { id -> Integer, }\n}\n#[derive(Queryable)]\npub struct Post { pub id: i32 }\n\"#;\n}\n";
        assert_eq!(scan_rust(src), Vec::new(), "fixture code inside r#\"...\"# is not a model");
    }

    #[test]
    fn python_docstring_samples_are_not_claimed_as_models() {
        let src = "import sqlalchemy\n\ndef helper():\n    \"\"\"\n    class Ghost(Base):\n        __tablename__ = 'ghosts'\n    \"\"\"\n    return 1\n";
        assert_eq!(scan_python(src), Vec::new(), "docstring sample is not a model");
    }

    #[test]
    fn apply_retags_existing_symbols_without_losing_line_ranges() {
        let mut symbols = vec![super::super::ExtractedSymbol {
            name: "User".to_string(),
            kind: "class".to_string(),
            start_line: 10,
            end_line: 40,
        }];
        apply(&[Detected { name: "User".to_string(), line: 10, kind: DATABASE_SCHEMA }], &mut symbols);
        assert_eq!(symbols[0].kind, DATABASE_SCHEMA);
        assert_eq!((symbols[0].start_line, symbols[0].end_line), (10, 40));
    }
}
