//! Checks each eqlib method's Rust declaration against its mangled name. The
//! link only checks the name; a wrong parameter type, a missing parameter or a
//! `*mut this` on a const method would link and then corrupt the stack or
//! the object. MSVC mangling encodes the full C++ signature, so this decodes
//! it and compares it with the Rust types.
//!
//! The decoder covers the part of MSVC mangling eqlib's methods use so far.
//! Anything else fails with "can't decode", and the decoder needs teaching.

use mq_emu_sys::eqlib::EQLIB_METHODS;

/// A C++ type, as far as the Rust side can tell types apart.
#[derive(Debug, Clone, PartialEq)]
enum Type {
    /// `void`, `int`, `bool` and so on, by their Rust name (`void` for `void`).
    Prim(&'static str),
    /// A class, struct or union by its innermost name.
    Class(String),
    /// An enum; MSVC stores them as `int`.
    Enum(String),
    Pointer {
        is_const: bool,
        to: Box<Type>,
    },
    Reference {
        is_const: bool,
        to: Box<Type>,
    },
}

impl Type {
    /// Every way the Rust side may write this type, as normalized tokens.
    fn rust_forms(&self) -> Vec<String> {
        match self {
            Type::Prim(name) => vec![(*name).to_owned()],
            Type::Class(name) => vec![name.clone()],
            Type::Enum(name) => vec!["i32".to_owned(), name.clone()],
            Type::Pointer { is_const, to } => pointers(*is_const, to, false),
            Type::Reference { is_const, to } => pointers(*is_const, to, true),
        }
    }
}

fn pointers(is_const: bool, to: &Type, or_reference: bool) -> Vec<String> {
    let mutability = if is_const { "const" } else { "mut" };
    let mut forms = Vec::new();
    for inner in to.rust_forms() {
        forms.push(format!("* {mutability} {inner}"));
        if or_reference {
            forms.push(if is_const { format!("& {inner}") } else { format!("& mut {inner}") });
        }
    }
    forms
}

/// A decoded non-static member function.
#[derive(Debug, PartialEq)]
struct Method {
    class: String,
    const_this: bool,
    /// `None` for constructors and destructors.
    returns: Option<Type>,
    params: Vec<Type>,
    is_constructor: bool,
}

struct Decoder<'a> {
    rest: &'a str,
    names: Vec<String>,
    param_types: Vec<Type>,
}

type Result<T> = std::result::Result<T, String>;

impl<'a> Decoder<'a> {
    fn next(&mut self) -> Result<char> {
        let c = self.rest.chars().next().ok_or("unexpected end")?;
        self.rest = &self.rest[1..];
        Ok(c)
    }

    fn eat(&mut self, prefix: &str) -> bool {
        match self.rest.strip_prefix(prefix) {
            Some(rest) => {
                self.rest = rest;
                true
            }
            None => false,
        }
    }

    fn fail<T>(&self, what: &str) -> Result<T> {
        Err(format!("can't decode {what} at {:?}", self.rest))
    }

    /// One name fragment: a back-reference digit, or an identifier ending in `@`.
    fn fragment(&mut self) -> Result<String> {
        if let Some(digit) = self.rest.chars().next().and_then(|c| c.to_digit(10)) {
            self.rest = &self.rest[1..];
            return self.names.get(digit as usize).cloned().ok_or_else(|| format!("bad name back-reference {digit}"));
        }
        if self.rest.starts_with('?') {
            return self.fail("special or template name");
        }
        let end = self.rest.find('@').ok_or("unterminated name")?;
        let name = self.rest[..end].to_owned();
        self.rest = &self.rest[end + 1..];
        if self.names.len() < 10 {
            self.names.push(name.clone());
        }
        Ok(name)
    }

    /// Name fragments, innermost first, up to the closing `@`.
    fn qualified_name(&mut self) -> Result<Vec<String>> {
        let mut fragments = Vec::new();
        while !self.eat("@") {
            fragments.push(self.fragment()?);
        }
        Ok(fragments)
    }

    fn pointee_const(&mut self) -> Result<bool> {
        match self.next()? {
            'A' => Ok(false),
            'B' => Ok(true),
            _ => self.fail("pointer qualifier"),
        }
    }

    fn ty(&mut self) -> Result<Type> {
        let prim = match self.rest.get(..2) {
            Some("_N") => Some("bool"),
            Some("_J") => Some("i64"),
            Some("_K") => Some("u64"),
            Some("_W") => Some("u16"),
            _ => None,
        };
        if let Some(prim) = prim {
            self.rest = &self.rest[2..];
            return Ok(Type::Prim(prim));
        }
        let c = self.next()?;
        Ok(match c {
            'X' => Type::Prim("void"),
            'C' | 'D' => Type::Prim("i8"),
            'E' => Type::Prim("u8"),
            'F' => Type::Prim("i16"),
            'G' => Type::Prim("u16"),
            'H' | 'J' => Type::Prim("i32"),
            'I' | 'K' => Type::Prim("u32"),
            'M' => Type::Prim("f32"),
            'N' => Type::Prim("f64"),
            // `Q` is a const pointer; the pointer's own constness doesn't matter to callers.
            'P' | 'Q' => Type::Pointer {
                is_const: self.pointee_const()?,
                to: Box::new(self.ty()?),
            },
            'A' => Type::Reference {
                is_const: self.pointee_const()?,
                to: Box::new(self.ty()?),
            },
            'U' | 'V' | 'T' => Type::Class(self.qualified_name()?.remove(0)),
            'W' if self.eat("4") => Type::Enum(self.qualified_name()?.remove(0)),
            _ => return Err(format!("can't decode type {c:?} before {:?}", self.rest)),
        })
    }

    /// A parameter: a back-reference digit, or a type (remembered if its code
    /// is longer than one character).
    fn param(&mut self) -> Result<Type> {
        if let Some(digit) = self.rest.chars().next().and_then(|c| c.to_digit(10)) {
            self.rest = &self.rest[1..];
            return self.param_types.get(digit as usize).cloned().ok_or_else(|| format!("bad parameter back-reference {digit}"));
        }
        let before = self.rest.len();
        let ty = self.ty()?;
        if before - self.rest.len() > 1 && self.param_types.len() < 10 {
            self.param_types.push(ty.clone());
        }
        Ok(ty)
    }
}

fn decode(mangled: &str) -> Result<Method> {
    let mut d = Decoder {
        rest: mangled,
        names: Vec::new(),
        param_types: Vec::new(),
    };
    if !d.eat("?") {
        return d.fail("non-C++ name");
    }
    let structor = if d.eat("?0") {
        Some(true)
    } else if d.eat("?1") {
        Some(false)
    } else {
        None
    };
    let mut fragments = d.qualified_name()?;
    if structor.is_none() {
        if fragments.len() < 2 {
            return d.fail("unscoped function");
        }
        fragments.remove(0); // the method's own name
    }
    let class = fragments.first().cloned().ok_or("no class name")?;
    match d.next()? {
        'Q' | 'A' | 'I' | 'U' | 'E' | 'M' => {}
        'S' | 'C' | 'K' => {
            return Err("a static member function: declare it under `functions`, not `methods`".to_owned());
        }
        'Y' => {
            return Err("a free function: declare it under `functions`, not `methods`".to_owned());
        }
        _ => return d.fail("access"),
    }
    let const_this = match d.next()? {
        'A' => false,
        'B' => true,
        _ => return d.fail("`this` qualifier"),
    };
    if d.next()? != 'E' {
        return Err("not __thiscall".to_owned());
    }
    let returns = if d.eat("@") {
        if structor.is_none() {
            return d.fail("missing return type");
        }
        None
    } else {
        // A class returned by value is prefixed with its storage class.
        let _ = d.eat("?A") || d.eat("?B");
        Some(d.ty()?)
    };
    let mut params = Vec::new();
    if !d.eat("X") {
        while !d.eat("@") {
            if d.rest.starts_with('Z') {
                return Err("variadic".to_owned());
            }
            params.push(d.param()?);
        }
    }
    if !d.eat("Z") || !d.rest.is_empty() {
        return d.fail("throw specification");
    }
    Ok(Method {
        class,
        const_this,
        returns,
        params,
        is_constructor: structor == Some(true),
    })
}

/// Splits Rust source text into tokens, with `::` as one token.
fn tokens(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c.is_alphanumeric() || c == '_' {
            let mut ident = c.to_string();
            while let Some(&n) = chars.peek().filter(|n| n.is_alphanumeric() || **n == '_') {
                ident.push(n);
                chars.next();
            }
            out.push(ident);
        } else if c == ':' && chars.peek() == Some(&':') {
            chars.next();
            out.push("::".to_owned());
        } else {
            out.push(c.to_string());
        }
    }
    out
}

/// A Rust type as tokens joined by spaces, without paths, with the `c_*`
/// aliases replaced by what they are on this target.
fn normalize(tokens: &[String]) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens.get(i + 1).is_some_and(|t| t == "::") {
            i += 2;
            continue;
        }
        out.push(match tokens[i].as_str() {
            "c_void" => "void",
            "c_char" | "c_schar" => "i8",
            "c_uchar" => "u8",
            "c_short" => "i16",
            "c_ushort" => "u16",
            "c_int" | "c_long" => "i32",
            "c_uint" | "c_ulong" => "u32",
            "c_longlong" => "i64",
            "c_ulonglong" => "u64",
            "c_float" => "f32",
            "c_double" => "f64",
            other => other,
        });
        i += 1;
    }
    out.join(" ")
}

/// The parameter types in a Rust parameter list (`name: Type, ...`).
fn rust_params(args: &str) -> Vec<String> {
    let tokens = tokens(args);
    let mut params = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut depth = 0;
    for token in tokens {
        match token.as_str() {
            "<" | "(" | "[" => depth += 1,
            ">" | ")" | "]" => depth -= 1,
            "," if depth == 0 => {
                params.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(token);
    }
    if !current.is_empty() {
        params.push(current);
    }
    params
        .into_iter()
        .map(|param| {
            // Drop `name :`.
            let ty = param.iter().position(|t| t == ":").map_or(&param[..], |colon| &param[colon + 1..]);
            normalize(ty)
        })
        .collect()
}

/// Compares a declaration with its mangled name; returns what's wrong.
fn check(mangled: &str, args: &str, ret: &str) -> Vec<String> {
    let method = match decode(mangled) {
        Ok(method) => method,
        Err(e) => return vec![e],
    };
    let mut problems = Vec::new();
    let mismatch = |what: String, actual: &str, ty: &Type| {
        let forms = ty.rust_forms();
        (!forms.iter().any(|f| f == actual)).then(|| format!("{what} is `{actual}`, C++ has {}", forms.join(" or ")))
    };
    let params = rust_params(args);
    let Some((this, params)) = params.split_first() else {
        return vec!["no `this` parameter".to_owned()];
    };
    let this_type = Type::Pointer {
        is_const: method.const_this,
        to: Box::new(Type::Class(method.class.clone())),
    };
    problems.extend(mismatch("`this`".to_owned(), this, &this_type));
    if params.len() != method.params.len() {
        problems.push(format!("{} parameters after `this`, C++ has {}", params.len(), method.params.len()));
    } else {
        for (i, (actual, ty)) in params.iter().zip(&method.params).enumerate() {
            problems.extend(mismatch(format!("parameter {}", i + 2), actual, ty));
        }
    }
    let ret = normalize(&tokens(ret));
    match &method.returns {
        Some(Type::Prim("void")) | None if method.is_constructor => {
            // A constructor returns `this` in EAX; declaring it is optional.
            if !ret.is_empty() {
                problems.extend(mismatch(
                    "the return type".to_owned(),
                    &ret,
                    &Type::Pointer {
                        is_const: false,
                        to: Box::new(Type::Class(method.class.clone())),
                    },
                ));
            }
        }
        Some(Type::Prim("void")) | None => {
            if !ret.is_empty() {
                problems.push(format!("returns `{ret}`, C++ returns nothing"));
            }
        }
        Some(ty) => problems.extend(mismatch("the return type".to_owned(), &ret, ty)),
    }
    problems
}

#[test]
fn every_eqlib_method_matches_its_mangled_signature() {
    let failures: Vec<String> = EQLIB_METHODS
        .iter()
        .filter_map(|&(mangled, args, ret)| {
            let problems = check(mangled, args, ret);
            (!problems.is_empty()).then(|| format!("{mangled}: {}", problems.join("; ")))
        })
        .collect();
    assert!(failures.is_empty(), "declarations that don't match eqlib:\n{}", failures.join("\n"));
}

#[test]
fn decodes_name_back_references_and_enum_returns() {
    let method = decode("?GetType@CXWnd@eqlib@@QBE?AW4UIType@2@XZ").unwrap();
    assert_eq!(
        method,
        Method {
            class: "CXWnd".into(),
            const_this: true,
            returns: Some(Type::Enum("UIType".into())),
            params: vec![],
            is_constructor: false
        }
    );
}

#[test]
fn decodes_parameter_back_references() {
    // Two `CXWnd*` parameters; the second is written as back-reference 0.
    let method = decode("?Swap@CXWnd@eqlib@@QAEXPAV12@0@Z").unwrap();
    let wnd = Type::Pointer {
        is_const: false,
        to: Box::new(Type::Class("CXWnd".into())),
    };
    assert_eq!(method.params, vec![wnd.clone(), wnd]);
}

#[test]
fn decodes_constructors_and_destructors() {
    let ctor = decode("??0CXStr@eqlib@@QAE@PBD@Z").unwrap();
    assert!(ctor.is_constructor && ctor.returns.is_none());
    assert_eq!(
        ctor.params,
        vec![Type::Pointer {
            is_const: true,
            to: Box::new(Type::Prim("i8"))
        }]
    );
    let dtor = decode("??1CXStr@eqlib@@QAE@XZ").unwrap();
    assert!(!dtor.is_constructor && dtor.params.is_empty());
}

#[test]
fn accepts_a_matching_declaration() {
    let problems = check("?Max_HP@CharacterZoneClient@eqlib@@QAEHH_N@Z", "this: *mut CharacterZoneClient, spawn_type: c_int, cap_at_max: bool", "c_int");
    assert!(problems.is_empty(), "{problems:?}");
}

// The checks below prove the test catches each kind of mistake.

#[test]
fn rejects_a_wrong_parameter_type() {
    let problems = check("?Max_HP@CharacterZoneClient@eqlib@@QAEHH_N@Z", "this: *mut CharacterZoneClient, spawn_type: c_int, cap_at_max: c_int", "c_int");
    assert_eq!(problems, ["parameter 3 is `i32`, C++ has bool"]);
}

#[test]
fn rejects_a_missing_parameter() {
    let problems = check("?Max_HP@CharacterZoneClient@eqlib@@QAEHH_N@Z", "this: *mut CharacterZoneClient, spawn_type: c_int", "c_int");
    assert_eq!(problems, ["1 parameters after `this`, C++ has 2"]);
}

#[test]
fn rejects_a_mutable_this_on_a_const_method() {
    let problems = check("?GetType@CXWnd@eqlib@@QBE?AW4UIType@2@XZ", "this: *mut CXWnd", "c_int");
    assert_eq!(problems, ["`this` is `* mut CXWnd`, C++ has * const CXWnd"]);
}

#[test]
fn rejects_the_wrong_class() {
    let problems = check("?GetType@CXWnd@eqlib@@QBE?AW4UIType@2@XZ", "this: *const CSidlScreenWnd", "c_int");
    assert_eq!(problems, ["`this` is `* const CSidlScreenWnd`, C++ has * const CXWnd"]);
}

#[test]
fn rejects_a_wrong_return_type() {
    let problems = check("??1CXStr@eqlib@@QAE@XZ", "this: *mut CXStr", "bool");
    assert_eq!(problems, ["returns `bool`, C++ returns nothing"]);
    let problems = check("?Max_HP@CharacterZoneClient@eqlib@@QAEHH_N@Z", "this: *mut CharacterZoneClient, spawn_type: c_int, cap_at_max: bool", "");
    assert_eq!(problems, ["the return type is ``, C++ has i32"]);
}

#[test]
fn rejects_static_and_free_functions() {
    assert_eq!(check("?Get@CXStr@eqlib@@SAHXZ", "this: *mut CXStr", ""), ["a static member function: declare it under `functions`, not `methods`"]);
    assert_eq!(check("?Get@eqlib@@YAHXZ", "this: *mut CXStr", ""), ["a free function: declare it under `functions`, not `methods`"]);
}

#[test]
fn rejects_what_it_cant_decode() {
    let problems = check("?Get@CXWnd@eqlib@@QAEX$$QAVCXStr@2@@Z", "this: *mut CXWnd, s: *mut CXStr", "");
    assert_eq!(problems.len(), 1);
    assert!(problems[0].starts_with("can't decode"), "{problems:?}");
}
