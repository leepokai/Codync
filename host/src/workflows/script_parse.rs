//! Parse a workflow script with boa and decide whether it is still a canvas.

#![allow(
    clippy::too_many_lines,
    clippy::map_unwrap_or,
    clippy::needless_pass_by_value,
    clippy::similar_names,
    clippy::enum_glob_use
)]

use super::{edge, field_map, json_str, node, order_edge};
use crate::workflows::graph::BotState;
use crate::workflows::limits::{
    MAX_DESCRIPTION, MAX_PHASE_TITLE, MAX_PHASES, MAX_SCRIPT_BYTES, MAX_TITLE, SCRIPT_PARSE_DEPTH,
};
use crate::workflows::model::{Branch, Graph, Node, NodeKind};
use anyhow::{Result, bail};
use boa_ast::declaration::{Binding, Declaration, ExportDeclaration, LexicalDeclaration};
use boa_ast::expression::access::{PropertyAccess, PropertyAccessField};
use boa_ast::expression::literal::{Literal, ObjectLiteral, PropertyDefinition};
use boa_ast::expression::operator::binary::{ArithmeticOp, BinaryOp, RelationalOp};
use boa_ast::expression::{Call, Expression, Identifier};
use boa_ast::function::FormalParameterList;
use boa_ast::property::PropertyName;
use boa_ast::statement::{If, Statement};
use boa_ast::{ModuleItem, StatementListItem};
use boa_interner::{Interner, Sym};
use boa_parser::{Parser, Source};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

pub struct Meta {
    pub name: String,
    #[allow(dead_code)]
    pub description: String,
    pub phases: Vec<String>,
}

pub enum Kind {
    Static(Vec<Step>),
    Dynamic,
}

pub struct Checked {
    pub meta: Meta,
    pub kind: Kind,
    /// Bot ids named by `agent()`. An empty id is a call whose bot is not a string literal.
    pub agents: Vec<String>,
}

pub struct Step {
    hint: Option<String>,
    kind: NodeKind,
    fields: Map<String, Value>,
    name: Option<String>,
    inputs: Vec<Input>,
}

enum Input {
    Args,
    Var(String),
    /// The step runs after `name` finishes. The earlier step's text does not become this step's input.
    Wait(String),
    Branch(Branch),
}

const PARAMS: [&str; 8] = ["agent", "parallel", "pipeline", "phase", "log", "args", "approval", "output"];
const ALLOWED: [&str; 16] = [
    "JSON",
    "Object",
    "Array",
    "String",
    "Number",
    "Boolean",
    "Promise",
    "Error",
    "TypeError",
    "RangeError",
    "Math",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "match",
];
const FORBIDDEN: [&str; 19] = [
    "eval",
    "Function",
    "require",
    "process",
    "Deno",
    "Bun",
    "fetch",
    "XMLHttpRequest",
    "WebSocket",
    "Date",
    "setTimeout",
    "setInterval",
    "queueMicrotask",
    "crypto",
    "globalThis",
    "Proxy",
    "Reflect",
    "WebAssembly",
    "RegExp",
];

pub fn meta_name(script: &str) -> Option<String> {
    let mut interner = Interner::default();
    let mut parser = Parser::new(Source::from_bytes(script.as_bytes()));
    let module = parser.parse_module(&boa_ast::scope::Scope::new_global(), &mut interner).ok()?;
    meta_of(&module, &interner).ok().map(|meta| meta.name)
}

pub fn phases_of(script: &str) -> Vec<String> {
    check(script, &|_| BotState::Ok).map(|c| c.meta.phases).unwrap_or_default()
}

pub fn check(script: &str, bots: &impl Fn(&str) -> BotState) -> Result<Checked> {
    if script.len() > MAX_SCRIPT_BYTES {
        bail!("This script is too long");
    }
    if !brackets_fit(script) {
        bail!("This script is nested too deeply");
    }
    let owned = script.to_owned();
    let built = std::thread::Builder::new()
        .name("workflow-parse".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || parse_checked(&owned))
        .map_err(|_| anyhow::anyhow!("This script couldn't be read"))?;
    let checked = match built.join() {
        Ok(result) => result?,
        Err(_) => bail!("This script is nested too deeply"),
    };
    for bot in &checked.agents {
        if !bot.is_empty() && !matches!(bots(bot), BotState::Ok) {
            bail!("Choose a bot for this step");
        }
    }
    Ok(checked)
}

fn parse_checked(script: &str) -> Result<Checked> {
    let mut interner = Interner::default();
    let mut parser = Parser::new(Source::from_bytes(script.as_bytes()));
    let module = parser
        .parse_module(&boa_ast::scope::Scope::new_global(), &mut interner)
        .map_err(|_| anyhow::anyhow!("This script couldn't be read"))?;
    let mut forbidden = Forbid { interner: &interner, declared: HashSet::new(), agents: Vec::new() };
    forbidden.module(&module)?;
    for (_, schema) in &forbidden.agents {
        if let Some(schema) = schema {
            validate_schema(schema)?;
        }
    }
    let meta = meta_of(&module, &interner)?;
    let body = function_body(&module, &interner)?;
    let hints = node_comments(script);
    let kind = match static_body(body, &interner, &hints) {
        Some(mut steps) => {
            assemble_fix_tasks(&mut steps);
            Kind::Static(steps)
        }
        None => Kind::Dynamic,
    };
    let agents = forbidden.agents.into_iter().map(|(bot, _)| bot).collect();
    Ok(Checked { meta, kind, agents })
}

/// Bracket depth outside strings and comments. The recursive parser never sees a deeper tree.
fn brackets_fit(script: &str) -> bool {
    let bytes = script.as_bytes();
    let mut i = 0;
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut line = false;
    let mut block = false;
    while i < bytes.len() {
        let byte = bytes[i];
        if line {
            if byte == b'\n' {
                line = false;
            }
            i += 1;
            continue;
        }
        if block {
            if byte == b'*' && bytes.get(i + 1) == Some(&b'/') {
                block = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if let Some(mark) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == mark {
                quote = None;
            } else if mark == b'`' && byte == b'$' && bytes.get(i + 1) == Some(&b'{') {
                quote = None;
                depth += 1;
                if depth > SCRIPT_PARSE_DEPTH {
                    return false;
                }
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if byte == b'/' && bytes.get(i + 1) == Some(&b'/') {
            line = true;
            i += 2;
            continue;
        }
        if byte == b'/' && bytes.get(i + 1) == Some(&b'*') {
            block = true;
            i += 2;
            continue;
        }
        if byte == b'\'' || byte == b'"' || byte == b'`' {
            quote = Some(byte);
            i += 1;
            continue;
        }
        if byte == b'(' || byte == b'[' || byte == b'{' {
            depth += 1;
            if depth > SCRIPT_PARSE_DEPTH {
                return false;
            }
        } else if byte == b')' || byte == b']' || byte == b'}' {
            depth = depth.saturating_sub(1);
        }
        i += 1;
    }
    true
}

pub fn assemble(previous: &Graph, steps: &[Step]) -> Graph {
    let mut kept: Vec<Node> =
        previous.nodes.iter().filter(|n| n.kind == NodeKind::Trigger || n.kind.is_annotation()).cloned().collect();
    if !kept.iter().any(|n| n.kind == NodeKind::Trigger) {
        kept.splice(0..0, crate::workflows::model::default_graph().nodes);
    }
    let old: HashMap<&str, &Node> = previous.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut names: HashMap<String, String> = HashMap::new();
    let mut body = Vec::new();
    let mut column = 0.0;
    for step in steps {
        let reuse = step.hint.as_deref().and_then(|id| old.get(id)).filter(|n| n.kind == step.kind);
        let id = reuse.map(|n| n.id.clone()).unwrap_or_else(crate::workflows::model::new_id);
        let at = if let Some(node) = reuse {
            node.at
        } else {
            column += 1.0;
            crate::workflows::model::Point { x: 480.0 + (column - 1.0) * 400.0, y: 80.0 }
        };
        if let Some(name) = &step.name {
            names.insert(name.clone(), id.clone());
        }
        body.push((id.clone(), step, at));
        let _ = id;
    }
    let mut nodes = kept;
    let mut edges = Vec::new();
    let mut args_nodes: Vec<String> = Vec::new();
    for (id, step, at) in &body {
        nodes.push(node(id.clone(), step.kind, at.x, at.y, step.fields.clone()));
        for input in &step.inputs {
            match input {
                Input::Args => {
                    if !args_nodes.contains(id) {
                        args_nodes.push(id.clone());
                    }
                }
                Input::Var(name) => {
                    if let Some(from) = names.get(name.as_str()) {
                        edges.push(edge(from, id, None));
                    }
                }
                Input::Wait(name) => {
                    if let Some(from) = names.get(name.as_str()) {
                        edges.push(order_edge(from, id));
                    }
                }
                Input::Branch(_) => {}
            }
        }
    }
    // Every step that reads the trigger payload gets that text, not only the first.
    for target in &args_nodes {
        for trigger in nodes.iter().filter(|n| n.kind == NodeKind::Trigger) {
            let linked =
                edges.iter().any(|edge| edge.from == trigger.id && edge.to == *target && !edge.id.ends_with(":order"));
            if !linked {
                edges.push(edge(&trigger.id, target, None));
            }
        }
    }
    let mut condition: Option<String> = None;
    for (id, step, _) in &body {
        if let Some(cond) = &condition
            && cond != id
        {
            for input in &step.inputs {
                if let Input::Branch(branch) = input {
                    edges.push(edge(cond, id, Some(*branch)));
                }
            }
        }
        if step.kind == NodeKind::Condition {
            condition = Some(id.clone());
        }
    }
    Graph { nodes, edges, strokes: previous.strokes.clone() }
}

struct Forbid<'a> {
    interner: &'a Interner,
    declared: HashSet<String>,
    agents: Vec<(String, Option<Value>)>,
}

impl Forbid<'_> {
    fn module(&mut self, module: &boa_ast::Module) -> Result<()> {
        for item in module.items().items() {
            match item {
                ModuleItem::ImportDeclaration(_) => bail!("This script uses \"import\", which isn't available"),
                ModuleItem::ExportDeclaration(export) => self.export(export)?,
                ModuleItem::StatementListItem(_) => bail!("This script couldn't be read"),
            }
        }
        Ok(())
    }

    fn export(&mut self, export: &ExportDeclaration) -> Result<()> {
        match export {
            ExportDeclaration::Declaration(Declaration::Lexical(LexicalDeclaration::Const(list))) => {
                for var in list.as_ref() {
                    if let Some(init) = var.init() {
                        self.expr(init)?;
                    }
                }
                Ok(())
            }
            ExportDeclaration::DefaultAsyncFunctionDeclaration(fun) => {
                self.params(fun.parameters())?;
                self.items(fun.body().statements())
            }
            _ => bail!("This script uses \"export\", which isn't available"),
        }
    }

    fn params(&mut self, list: &FormalParameterList) -> Result<()> {
        let params = list.as_ref();
        if params.len() != 1 || params[0].is_rest_param() || params[0].init().is_some() {
            bail!("This script couldn't be read");
        }
        let Binding::Pattern(boa_ast::pattern::Pattern::Object(pattern)) = params[0].variable().binding() else {
            bail!("This script couldn't be read");
        };
        for element in pattern.bindings() {
            let boa_ast::pattern::ObjectPatternElement::SingleName { name, ident, default_init } = element else {
                bail!("This script couldn't be read");
            };
            if default_init.is_some() {
                bail!("This script couldn't be read");
            }
            let PropertyName::Literal(sym) = name else { bail!("This script couldn't be read") };
            let key = self.text(*sym);
            if !PARAMS.contains(&key.as_str()) || self.text(ident.sym()) != key {
                bail!("This script couldn't be read");
            }
            self.declared.insert(key);
        }
        Ok(())
    }

    fn items(&mut self, items: &[StatementListItem]) -> Result<()> {
        for item in items {
            match item {
                StatementListItem::Statement(statement) => self.stmt(statement)?,
                StatementListItem::Declaration(Declaration::Lexical(lex)) => {
                    let list = lex.variable_list();
                    for var in list.as_ref() {
                        if let Some(init) = var.init() {
                            self.expr(init)?;
                        }
                        if let Binding::Identifier(id) = var.binding() {
                            self.declared.insert(self.text(id.sym()));
                        }
                    }
                }
                StatementListItem::Declaration(_) => bail!("This script uses \"Function\", which isn't available"),
            }
        }
        Ok(())
    }

    fn stmt(&mut self, statement: &Statement) -> Result<()> {
        match statement {
            Statement::Block(block) => self.items(block.statement_list().statements()),
            Statement::If(stmt) => {
                self.expr(stmt.cond())?;
                self.stmt(stmt.body())?;
                if let Some(else_node) = stmt.else_node() {
                    self.stmt(else_node)?;
                }
                Ok(())
            }
            Statement::Return(ret) => {
                if let Some(target) = ret.target() {
                    self.expr(target)?;
                }
                Ok(())
            }
            Statement::Expression(expr) => self.expr(expr),
            Statement::Empty => Ok(()),
            Statement::ForLoop(loop_) => self.for_loop(loop_),
            Statement::ForInLoop(loop_) => {
                self.expr(loop_.target())?;
                self.stmt(loop_.body())
            }
            Statement::ForOfLoop(loop_) => {
                self.expr(loop_.iterable())?;
                self.stmt(loop_.body())
            }
            Statement::Try(node) => {
                self.items(node.block().statement_list().statements())?;
                if let Some(catch) = node.catch() {
                    self.items(catch.block().statement_list().statements())?;
                }
                if let Some(finally) = node.finally() {
                    self.items(finally.block().statement_list().statements())?;
                }
                Ok(())
            }
            Statement::Switch(node) => {
                self.expr(node.val())?;
                for case in node.cases() {
                    if let Some(condition) = case.condition() {
                        self.expr(condition)?;
                    }
                    self.items(case.body().statements())?;
                }
                Ok(())
            }
            _ => {
                self.walk_stmt_loose(statement)?;
                Ok(())
            }
        }
    }

    fn for_loop(&mut self, loop_: &boa_ast::statement::iteration::ForLoop) -> Result<()> {
        if let Some(init) = loop_.init() {
            match init {
                boa_ast::statement::iteration::ForLoopInitializer::Expression(expr) => self.expr(expr)?,
                boa_ast::statement::iteration::ForLoopInitializer::Var(var) => {
                    for item in var.0.as_ref() {
                        if let Some(init) = item.init() {
                            self.expr(init)?;
                        }
                    }
                }
                boa_ast::statement::iteration::ForLoopInitializer::Lexical(lex) => {
                    for item in lex.declaration().variable_list().as_ref() {
                        if let Some(init) = item.init() {
                            self.expr(init)?;
                        }
                    }
                }
            }
        }
        if let Some(condition) = loop_.condition() {
            self.expr(condition)?;
        }
        if let Some(final_expr) = loop_.final_expr() {
            self.expr(final_expr)?;
        }
        self.stmt(loop_.body())
    }

    /// A few statement shapes this file does not model. The walk is not the sandbox: loop, time,
    /// and memory limits on the script process are.
    fn walk_stmt_loose(&mut self, statement: &Statement) -> Result<()> {
        match statement {
            Statement::Var(var) => {
                for item in var.0.as_ref() {
                    if let Some(init) = item.init() {
                        self.expr(init)?;
                    }
                }
                Ok(())
            }
            Statement::Throw(throw) => self.expr(throw.target()),
            Statement::WhileLoop(loop_) => {
                self.expr(loop_.condition())?;
                self.stmt(loop_.body())
            }
            Statement::DoWhileLoop(loop_) => {
                self.stmt(loop_.body())?;
                self.expr(loop_.cond())
            }
            _ => Ok(()),
        }
    }

    fn expr(&mut self, expr: &Expression) -> Result<()> {
        match expr {
            Expression::Identifier(id) => self.ident(*id),
            Expression::RegExpLiteral(_) => bail!("This script uses \"RegExp\", which isn't available"),
            Expression::ArrayLiteral(array) => {
                for item in array.as_ref().iter().flatten() {
                    self.expr(item)?;
                }
                Ok(())
            }
            Expression::ObjectLiteral(object) => self.object(object),
            Expression::Call(call) => self.call(call),
            Expression::Await(wait) => self.expr(wait.target()),
            Expression::PropertyAccess(access) => self.access(access),
            Expression::Binary(binary) => {
                self.expr(binary.lhs())?;
                self.expr(binary.rhs())
            }
            Expression::Unary(unary) => self.expr(unary.target()),
            Expression::Conditional(cond) => {
                self.expr(cond.condition())?;
                self.expr(cond.if_true())?;
                self.expr(cond.if_false())
            }
            Expression::ArrowFunction(arrow) => self.fn_body(arrow.parameters(), arrow.body().statements()),
            Expression::AsyncArrowFunction(arrow) => self.fn_body(arrow.parameters(), arrow.body().statements()),
            Expression::FunctionExpression(fun) => self.fn_body(fun.parameters(), fun.body().statements()),
            Expression::AsyncFunctionExpression(fun) => self.fn_body(fun.parameters(), fun.body().statements()),
            Expression::New(new) => {
                self.expr(new.constructor())?;
                for arg in new.arguments() {
                    self.expr(arg)?;
                }
                Ok(())
            }
            Expression::TemplateLiteral(template) => {
                for element in template.elements() {
                    if let boa_ast::expression::literal::TemplateElement::Expr(expr) = element {
                        self.expr(expr)?;
                    }
                }
                Ok(())
            }
            Expression::Parenthesized(inner) => self.expr(inner.expression()),
            Expression::This => bail!("This script uses \"globalThis\", which isn't available"),
            _ => Ok(()),
        }
    }

    fn fn_body(&mut self, params: &FormalParameterList, body: &[StatementListItem]) -> Result<()> {
        let saved = self.declared.clone();
        for param in params.as_ref() {
            if let Binding::Identifier(id) = param.variable().binding() {
                self.declared.insert(self.text(id.sym()));
            }
        }
        self.items(body)?;
        self.declared = saved;
        Ok(())
    }

    fn object(&mut self, object: &ObjectLiteral) -> Result<()> {
        for prop in object.properties() {
            if let PropertyDefinition::Property(_, expr) = prop {
                self.expr(expr)?;
            }
        }
        Ok(())
    }

    fn access(&mut self, access: &PropertyAccess) -> Result<()> {
        let PropertyAccess::Simple(simple) = access else {
            return Ok(());
        };
        self.expr(simple.target())?;
        if let (Expression::Identifier(id), PropertyAccessField::Const(field)) = (simple.target(), simple.field())
            && self.text(id.sym()) == "Math"
            && self.text(*field) == "random"
        {
            bail!("This script uses \"Math.random\", which isn't available");
        }
        Ok(())
    }

    fn call(&mut self, call: &Call) -> Result<()> {
        if let Expression::Identifier(id) = call.function() {
            let name = self.text(id.sym());
            if name == "agent" {
                let bot = call.args().first().and_then(|arg| string_lit(arg, self.interner)).unwrap_or_default();
                let schema = call.args().get(2).and_then(|arg| object_schema(arg, self.interner));
                self.agents.push((bot, schema));
            }
            if name == "workflow" || name == "budget" {
                bail!("This script uses \"{name}\", which isn't available");
            }
        }
        self.expr(call.function())?;
        for arg in call.args() {
            self.expr(arg)?;
        }
        Ok(())
    }

    fn ident(&mut self, id: Identifier) -> Result<()> {
        let name = self.text(id.sym());
        if FORBIDDEN.contains(&name.as_str()) || name == "workflow" || name == "budget" {
            bail!("This script uses \"{name}\", which isn't available");
        }
        if self.declared.contains(&name) || ALLOWED.contains(&name.as_str()) {
            return Ok(());
        }
        bail!("This script uses \"{name}\", which isn't available");
    }

    fn text(&self, sym: Sym) -> String {
        self.interner.resolve_expect(sym).utf8().unwrap_or("").to_owned()
    }
}

fn meta_of(module: &boa_ast::Module, interner: &Interner) -> Result<Meta> {
    let mut found = None;
    for item in module.items().items() {
        let ModuleItem::ExportDeclaration(ExportDeclaration::Declaration(Declaration::Lexical(
            LexicalDeclaration::Const(list),
        ))) = item
        else {
            continue;
        };
        for var in list.as_ref() {
            let Binding::Identifier(id) = var.binding() else { continue };
            if interner.resolve_expect(id.sym()).utf8() != Some("meta") {
                continue;
            }
            let Some(Expression::ObjectLiteral(object)) = var.init() else {
                bail!("The workflow meta must be a literal");
            };
            found = Some(read_meta(object, interner)?);
        }
    }
    found.ok_or_else(|| anyhow::anyhow!("The workflow meta must be a literal"))
}

fn read_meta(object: &ObjectLiteral, interner: &Interner) -> Result<Meta> {
    let mut name = None;
    let mut description = String::new();
    let mut phases = Vec::new();
    for prop in object.properties() {
        let PropertyDefinition::Property(key, expr) = prop else {
            bail!("The workflow meta must be a literal");
        };
        let Some(sym) = key.literal() else { bail!("The workflow meta must be a literal") };
        let key = interner.resolve_expect(sym).utf8().unwrap_or("");
        match key {
            "name" => name = Some(expect_string(expr, interner)?),
            "description" => description = expect_string(expr, interner)?,
            "phases" => phases = expect_phases(expr, interner)?,
            _ => bail!("The workflow meta must be a literal"),
        }
    }
    let Some(name) = name.filter(|n| !n.is_empty() && n.chars().count() <= MAX_TITLE) else {
        bail!("The workflow meta must be a literal");
    };
    if description.chars().count() > MAX_DESCRIPTION || phases.len() > MAX_PHASES {
        bail!("The workflow meta must be a literal");
    }
    Ok(Meta { name, description, phases })
}

fn expect_phases(expr: &Expression, interner: &Interner) -> Result<Vec<String>> {
    let Expression::ArrayLiteral(array) = expr else { bail!("The workflow meta must be a literal") };
    let mut titles = Vec::new();
    for element in array.as_ref() {
        let Some(Expression::ObjectLiteral(object)) = element else { bail!("The workflow meta must be a literal") };
        let mut title = None;
        for prop in object.properties() {
            let PropertyDefinition::Property(key, value) = prop else {
                bail!("The workflow meta must be a literal");
            };
            if key.literal().and_then(|sym| interner.resolve_expect(sym).utf8()) != Some("title") {
                bail!("The workflow meta must be a literal");
            }
            title = Some(expect_string(value, interner)?);
        }
        let Some(title) = title.filter(|t| !t.is_empty() && t.chars().count() <= MAX_PHASE_TITLE) else {
            bail!("The workflow meta must be a literal");
        };
        titles.push(title);
    }
    Ok(titles)
}

fn expect_string(expr: &Expression, interner: &Interner) -> Result<String> {
    string_lit(expr, interner).ok_or_else(|| anyhow::anyhow!("The workflow meta must be a literal"))
}

fn string_lit(expr: &Expression, interner: &Interner) -> Option<String> {
    let Expression::Literal(Literal::String(sym)) = expr else { return None };
    Some(interner.resolve_expect(*sym).utf8()?.to_owned())
}

fn function_body<'a>(module: &'a boa_ast::Module, interner: &Interner) -> Result<&'a [StatementListItem]> {
    let mut found = None;
    for item in module.items().items() {
        if let ModuleItem::ExportDeclaration(ExportDeclaration::DefaultAsyncFunctionDeclaration(fun)) = item {
            let params = fun.parameters().as_ref();
            if params.len() != 1 {
                bail!("This script couldn't be read");
            }
            let _ = interner;
            found = Some(fun.body().statements());
        }
    }
    found.ok_or_else(|| anyhow::anyhow!("This script couldn't be read"))
}

fn static_body(items: &[StatementListItem], interner: &Interner, hints: &[String]) -> Option<Vec<Step>> {
    let mut steps = Vec::new();
    let mut hint = 0usize;
    let mut names = HashSet::new();
    let mut gates = Vec::new();
    let mut order = Order::default();
    if !static_items(items, interner, hints, &mut hint, &mut names, &mut steps, None, &mut gates, &mut order) {
        return None;
    }
    Some(steps)
}

#[allow(clippy::too_many_arguments)]
fn static_items(
    items: &[StatementListItem],
    interner: &Interner,
    hints: &[String],
    hint: &mut usize,
    names: &mut HashSet<String>,
    steps: &mut Vec<Step>,
    branch: Option<Branch>,
    gates: &mut Vec<String>,
    order: &mut Order,
) -> bool {
    for item in items {
        match item {
            StatementListItem::Declaration(Declaration::Lexical(LexicalDeclaration::Const(list))) => {
                if list.as_ref().len() != 1 {
                    return false;
                }
                let var = &list.as_ref()[0];
                let Binding::Identifier(id) = var.binding() else { return false };
                let name = text(interner, id.sym());
                let Some(init) = var.init() else { return false };
                let id_hint = take_hint(hints, hint);
                if !static_const(&name, init, interner, id_hint, names, steps, branch, gates, order) {
                    return false;
                }
                names.insert(name);
            }
            StatementListItem::Statement(Statement::Expression(expr)) => {
                let id_hint = take_hint(hints, hint);
                if !static_effect(expr, interner, id_hint, names, steps, branch, gates, order) {
                    return false;
                }
            }
            StatementListItem::Statement(Statement::If(stmt)) => {
                if !static_if(stmt, interner, hints, hint, names, steps, branch, gates, order) {
                    return false;
                }
            }
            StatementListItem::Statement(Statement::Return(_)) => return true,
            StatementListItem::Statement(Statement::Empty) => {}
            _ => return false,
        }
    }
    true
}

fn take_hint(hints: &[String], hint: &mut usize) -> Option<String> {
    let id = hints.get(*hint).cloned();
    if id.is_some() {
        *hint += 1;
    }
    id
}

#[allow(clippy::too_many_arguments)]
fn static_const(
    name: &str,
    init: &Expression,
    interner: &Interner,
    hint: Option<String>,
    names: &HashSet<String>,
    steps: &mut Vec<Step>,
    branch: Option<Branch>,
    gates: &mut Vec<String>,
    order: &mut Order,
) -> bool {
    if let Some(text) = string_lit(init, interner) {
        // A prompt has no incoming branch. That shape stays on the script runner.
        if branch.is_some() {
            return false;
        }
        return record(
            Step {
                hint,
                kind: NodeKind::Prompt,
                fields: field_map(&[("text", json_str(&text))]),
                name: Some(name.to_owned()),
                inputs: Vec::new(),
            },
            steps,
            gates,
            order,
        );
    }
    let Expression::Await(wait) = init else { return false };
    let Expression::Call(call) = wait.target() else { return false };
    let Expression::Identifier(id) = call.function() else { return false };
    if text(interner, id.sym()) != "agent" || call.args().len() < 2 || call.args().len() > 3 {
        return false;
    }
    let Some(bot) = string_lit(&call.args()[0], interner) else { return false };
    let Some(text_in) = text_input(&call.args()[1], interner, names) else { return false };
    if call.args().get(2).is_some_and(|arg| !label_only(arg, interner)) {
        return false;
    }
    let mut inputs = text_in;
    if let Some(branch) = branch {
        inputs.push(Input::Branch(branch));
    }
    let task = task_of(&inputs);
    record(
        Step {
            hint,
            kind: NodeKind::Agent,
            fields: field_map(&[("botId", json_str(&bot)), ("task", json_str(&task))]),
            name: Some(name.to_owned()),
            inputs,
        },
        steps,
        gates,
        order,
    )
}

/// The step this block last awaited, and whether a condition branch owns it.
#[derive(Clone, Default)]
struct Order {
    prev: Option<String>,
    /// A condition branch produced an effect. A later step cannot wait for both sides.
    alt: bool,
    effects: u32,
}

/// An awaited step waits for the awaited step before it. A condition's two sides stay
/// independent; a step after that condition leaves the script on the script runner.
fn record(mut step: Step, steps: &mut Vec<Step>, gates: &mut Vec<String>, order: &mut Order) -> bool {
    if order.alt {
        return false;
    }
    let effect = matches!(step.kind, NodeKind::Agent | NodeKind::Output | NodeKind::Approval);
    if effect {
        if step.kind == NodeKind::Approval && step.name.is_none() {
            step.name = Some(format!("__approval_{}", steps.len()));
        } else if step.name.is_none() {
            step.name = Some(format!("::step{}", steps.len()));
        }
        if let Some(name) = order.prev.clone()
            && !step.inputs.iter().any(|input| matches!(input, Input::Var(got) | Input::Wait(got) if got == &name))
        {
            step.inputs.insert(0, Input::Wait(name));
        }
        order.effects += 1;
    }
    let own = step.name.clone();
    for gate in gates.iter() {
        if own.as_deref() == Some(gate.as_str()) {
            continue;
        }
        if !step.inputs.iter().any(|input| matches!(input, Input::Var(name) | Input::Wait(name) if name == gate)) {
            step.inputs.push(Input::Wait(gate.clone()));
        }
    }
    if step.kind == NodeKind::Approval
        && let Some(name) = &step.name
        && !gates.iter().any(|gate| gate == name)
    {
        gates.push(name.clone());
    }
    if let Some(name) = step.name.clone() {
        order.prev = Some(name);
    }
    steps.push(step);
    true
}

fn task_of(inputs: &[Input]) -> String {
    let _ = inputs;
    String::new()
}

fn text_input(expr: &Expression, interner: &Interner, names: &HashSet<String>) -> Option<Vec<Input>> {
    if let Some(text) = string_lit(expr, interner) {
        return Some(vec![Input::Var(format!("__lit__{text}"))]);
    }
    if is_args(expr, interner) {
        return Some(vec![Input::Args]);
    }
    if let Expression::Identifier(id) = expr {
        let name = text(interner, id.sym());
        if names.contains(&name) {
            return Some(vec![Input::Var(name)]);
        }
    }
    if let Some((left, right)) = plus(expr) {
        if let Some(literal) = string_lit(left, interner) {
            if is_args(right, interner) {
                return Some(vec![Input::Var(format!("__task__{literal}")), Input::Args]);
            }
            if let Expression::Identifier(id) = right {
                let name = text(interner, id.sym());
                if names.contains(&name) {
                    return Some(vec![Input::Var(format!("__task__{literal}")), Input::Var(name)]);
                }
            }
        }
        if let Expression::Identifier(id) = left {
            let name = text(interner, id.sym());
            if names.contains(&name) && is_args(right, interner) {
                return Some(vec![Input::Var(name), Input::Args]);
            }
        }
        if let Some(parts) = fan_in(expr, interner, names) {
            return Some(parts);
        }
    }
    None
}

fn fan_in(expr: &Expression, interner: &Interner, names: &HashSet<String>) -> Option<Vec<Input>> {
    let mut flat = Vec::new();
    flatten_plus(expr, &mut flat);
    if flat.len() < 4 || !flat.len().is_multiple_of(2) {
        return None;
    }
    let mut inputs = Vec::new();
    let mut index = 1;
    let mut i = 0;
    while i < flat.len() {
        let literal = string_lit(flat[i], interner)?;
        let expected = if index == 1 { format!("Input {index}:\n") } else { format!("\n\nInput {index}:\n") };
        if literal != expected {
            return None;
        }
        i += 1;
        if i >= flat.len() {
            return None;
        }
        if is_args(flat[i], interner) {
            inputs.push(Input::Args);
        } else if let Expression::Identifier(id) = flat[i] {
            let name = text(interner, id.sym());
            if !names.contains(&name) {
                return None;
            }
            inputs.push(Input::Var(name));
        } else {
            return None;
        }
        i += 1;
        index += 1;
    }
    Some(inputs)
}

fn flatten_plus<'a>(expr: &'a Expression, out: &mut Vec<&'a Expression>) {
    if let Some((left, right)) = plus(expr) {
        flatten_plus(left, out);
        flatten_plus(right, out);
    } else {
        out.push(expr);
    }
}

fn plus(expr: &Expression) -> Option<(&Expression, &Expression)> {
    let Expression::Binary(binary) = expr else { return None };
    if binary.op() != BinaryOp::Arithmetic(ArithmeticOp::Add) {
        return None;
    }
    Some((binary.lhs(), binary.rhs()))
}

fn is_args(expr: &Expression, interner: &Interner) -> bool {
    let Expression::PropertyAccess(PropertyAccess::Simple(access)) = expr else { return false };
    let Expression::Identifier(id) = access.target() else { return false };
    let PropertyAccessField::Const(field) = access.field() else { return false };
    text(interner, id.sym()) == "args" && text(interner, *field) == "input"
}

fn label_only(expr: &Expression, interner: &Interner) -> bool {
    let Expression::ObjectLiteral(object) = expr else { return false };
    let props = object.properties();
    if props.len() != 1 {
        return false;
    }
    let PropertyDefinition::Property(key, _) = &props[0] else { return false };
    key.literal().and_then(|sym| interner.resolve_expect(sym).utf8()) == Some("label")
}

#[allow(clippy::too_many_arguments)]
fn static_effect(
    expr: &Expression,
    interner: &Interner,
    hint: Option<String>,
    names: &HashSet<String>,
    steps: &mut Vec<Step>,
    branch: Option<Branch>,
    gates: &mut Vec<String>,
    order: &mut Order,
) -> bool {
    let Expression::Await(wait) = expr else { return false };
    let Expression::Call(call) = wait.target() else { return false };
    let Expression::Identifier(id) = call.function() else { return false };
    let name = text(interner, id.sym());
    let mut inputs = Vec::new();
    if let Some(branch) = branch {
        inputs.push(Input::Branch(branch));
    }
    if name == "approval" && call.args().len() == 1 {
        let Some(question) = string_lit(&call.args()[0], interner) else { return false };
        return record(
            Step {
                hint,
                kind: NodeKind::Approval,
                fields: field_map(&[("question", json_str(&question))]),
                name: None,
                inputs,
            },
            steps,
            gates,
            order,
        );
    }
    if name == "output" && call.args().len() == 1 {
        let Some((target, bot, text_in)) = output_spec(&call.args()[0], interner, names) else { return false };
        inputs.extend(text_in);
        let mut fields = field_map(&[("target", json_str(&target))]);
        if let Some(bot) = bot {
            fields.insert("botId".into(), json_str(&bot));
        }
        return record(Step { hint, kind: NodeKind::Output, fields, name: None, inputs }, steps, gates, order);
    }
    false
}

fn output_spec(
    expr: &Expression,
    interner: &Interner,
    names: &HashSet<String>,
) -> Option<(String, Option<String>, Vec<Input>)> {
    let Expression::ObjectLiteral(object) = expr else { return None };
    let mut target = None;
    let mut bot = None;
    let mut text_in = None;
    for prop in object.properties() {
        let PropertyDefinition::Property(key, value) = prop else { return None };
        let key = key.literal().and_then(|sym| interner.resolve_expect(sym).utf8())?;
        match key {
            "target" => target = string_lit(value, interner),
            "botId" => bot = string_lit(value, interner),
            "text" => text_in = text_input(value, interner, names),
            _ => return None,
        }
    }
    let target = target?;
    if target != "notify" && target != "chat" {
        return None;
    }
    Some((target, bot, text_in?))
}

#[allow(clippy::too_many_arguments)]
fn static_if(
    stmt: &If,
    interner: &Interner,
    hints: &[String],
    hint: &mut usize,
    names: &mut HashSet<String>,
    steps: &mut Vec<Step>,
    branch: Option<Branch>,
    gates: &mut Vec<String>,
    order: &mut Order,
) -> bool {
    let Some((probe, op, value)) = condition(stmt.cond(), interner, names) else { return false };
    let id_hint = take_hint(hints, hint);
    let mut inputs = vec![Input::Var(probe)];
    if let Some(branch) = branch {
        inputs.push(Input::Branch(branch));
    }
    if !record(
        Step {
            hint: id_hint,
            kind: NodeKind::Condition,
            fields: field_map(&[("op", json_str(&op)), ("value", json_str(&value))]),
            name: None,
            inputs,
        },
        steps,
        gates,
        order,
    ) {
        return false;
    }
    let Statement::Block(yes) = stmt.body() else { return false };
    let incoming_gates = gates.clone();
    let incoming = order.clone();
    let mut yes_gates = incoming_gates.clone();
    let mut yes_order = incoming.clone();
    if !static_items(
        yes.statement_list().statements(),
        interner,
        hints,
        hint,
        names,
        steps,
        Some(Branch::Yes),
        &mut yes_gates,
        &mut yes_order,
    ) {
        return false;
    }
    let mut no_gates = incoming_gates;
    let mut no_order = incoming.clone();
    let branched = match stmt.else_node() {
        Some(Statement::Block(no)) => static_items(
            no.statement_list().statements(),
            interner,
            hints,
            hint,
            names,
            steps,
            Some(Branch::No),
            &mut no_gates,
            &mut no_order,
        ),
        Some(Statement::If(inner)) => {
            static_if(inner, interner, hints, hint, names, steps, Some(Branch::No), &mut no_gates, &mut no_order)
        }
        None => {
            no_gates.clone_from(&yes_gates);
            true
        }
        _ => false,
    };
    if !branched {
        return false;
    }
    // An approval that exists on only one side cannot be a line on the canvas.
    if yes_gates != no_gates {
        return false;
    }
    *gates = yes_gates;
    let grew = yes_order.effects > incoming.effects || no_order.effects > incoming.effects;
    *order = yes_order;
    if grew {
        order.alt = true;
    }
    true
}

fn condition(expr: &Expression, interner: &Interner, names: &HashSet<String>) -> Option<(String, String, String)> {
    if let Expression::Call(call) = expr {
        let Expression::Identifier(id) = call.function() else { return None };
        if text(interner, id.sym()) == "match" && call.args().len() == 2 {
            let probe = bound_name(&call.args()[0], names, interner)?;
            let value = string_lit(&call.args()[1], interner)?;
            return Some((probe, "regex".into(), value));
        }
        let Expression::PropertyAccess(PropertyAccess::Simple(access)) = call.function() else { return None };
        if let PropertyAccessField::Const(field) = access.field()
            && text(interner, *field) == "includes"
            && call.args().len() == 1
        {
            let probe = bound_name(access.target(), names, interner)?;
            let value = string_lit(&call.args()[0], interner)?;
            return Some((probe, "contains".into(), value));
        }
    }
    let Expression::Binary(binary) = expr else { return None };
    if binary.op() != BinaryOp::Relational(RelationalOp::StrictEqual) {
        return None;
    }
    if let Some(value) = string_lit(binary.rhs(), interner)
        && let Some(probe) = bound_name(binary.lhs(), names, interner)
    {
        return Some((probe, "equals".into(), value));
    }
    if string_lit(binary.rhs(), interner).as_deref() == Some("")
        && let Expression::Call(call) = binary.lhs()
    {
        let Expression::PropertyAccess(PropertyAccess::Simple(access)) = call.function() else { return None };
        let PropertyAccessField::Const(field) = access.field() else { return None };
        if text(interner, *field) == "trim" && call.args().is_empty() {
            let probe = bound_name(access.target(), names, interner)?;
            return Some((probe, "empty".into(), String::new()));
        }
    }
    None
}

fn bound_name(expr: &Expression, names: &HashSet<String>, interner: &Interner) -> Option<String> {
    let Expression::Identifier(id) = expr else { return None };
    let name = text(interner, id.sym());
    names.contains(&name).then_some(name)
}

fn text(interner: &Interner, sym: Sym) -> String {
    interner.resolve_expect(sym).utf8().unwrap_or("").to_owned()
}

fn node_comments(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
            let start = i + 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            let comment = src[start..i].trim();
            if let Some(id) = comment.strip_prefix("@node ") {
                let id = id.trim();
                if !id.is_empty() && !id.contains(char::is_whitespace) {
                    out.push(id.to_owned());
                }
            }
            continue;
        }
        i += 1;
    }
    out
}

fn object_schema(expr: &Expression, interner: &Interner) -> Option<Value> {
    let Expression::ObjectLiteral(object) = expr else { return None };
    for prop in object.properties() {
        let PropertyDefinition::Property(key, value) = prop else { continue };
        if key.literal().and_then(|sym| interner.resolve_expect(sym).utf8()) == Some("schema") {
            return literal_json(value, interner);
        }
    }
    None
}

fn literal_json(expr: &Expression, interner: &Interner) -> Option<Value> {
    match expr {
        Expression::Literal(Literal::String(sym)) => Some(json!(interner.resolve_expect(*sym).utf8()?)),
        Expression::Literal(Literal::Num(n)) => serde_json::Number::from_f64(*n).map(Value::Number),
        Expression::Literal(Literal::Int(n)) => Some(json!(n)),
        Expression::Literal(Literal::Bool(b)) => Some(json!(b)),
        Expression::Literal(Literal::Null) => Some(Value::Null),
        Expression::ArrayLiteral(array) => {
            let mut items = Vec::new();
            for element in array.as_ref() {
                items.push(literal_json(element.as_ref()?, interner)?);
            }
            Some(Value::Array(items))
        }
        Expression::ObjectLiteral(object) => {
            let mut map = Map::new();
            for prop in object.properties() {
                let PropertyDefinition::Property(key, value) = prop else { return None };
                let key = key.literal().and_then(|sym| interner.resolve_expect(sym).utf8())?.to_owned();
                map.insert(key, literal_json(value, interner)?);
            }
            Some(Value::Object(map))
        }
        _ => None,
    }
}

fn validate_schema(value: &Value) -> Result<()> {
    let obj = value.as_object().ok_or_else(|| anyhow::anyhow!("This step's schema isn't supported"))?;
    for key in obj.keys() {
        if !matches!(key.as_str(), "type" | "properties" | "required" | "items" | "enum") {
            bail!("This step's schema isn't supported");
        }
    }
    if let Some(kind) = obj.get("type").and_then(Value::as_str)
        && !matches!(kind, "object" | "array" | "string" | "number" | "boolean" | "null")
    {
        bail!("This step's schema isn't supported");
    }
    if let Some(props) = obj.get("properties") {
        let props = props.as_object().ok_or_else(|| anyhow::anyhow!("This step's schema isn't supported"))?;
        for prop in props.values() {
            validate_schema(prop)?;
        }
    }
    if let Some(items) = obj.get("items") {
        validate_schema(items)?;
    }
    Ok(())
}

fn assemble_fix_tasks(steps: &mut [Step]) {
    for step in steps {
        let task = step
            .inputs
            .iter()
            .find_map(|input| match input {
                Input::Var(name) if name.starts_with("__task__") => {
                    Some(name.trim_start_matches("__task__").to_owned())
                }
                Input::Var(name) if name.starts_with("__lit__") => Some(name.trim_start_matches("__lit__").to_owned()),
                _ => None,
            })
            .unwrap_or_default();
        let task = task.strip_suffix("\n\n").unwrap_or(&task).to_owned();
        if step.kind == NodeKind::Agent {
            step.fields.insert("task".into(), json_str(&task));
        }
        if step.kind == NodeKind::Output && !task.is_empty() {
            step.fields.insert("text".into(), json_str(&task));
        }
        step.inputs.retain(
            |input| !matches!(input, Input::Var(name) if name.starts_with("__task__") || name.starts_with("__lit__")),
        );
    }
}
