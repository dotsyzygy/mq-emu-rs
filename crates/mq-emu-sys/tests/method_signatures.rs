//! Checks each eqlib method's and C++ function's Rust declaration against its
//! mangled name. The
//! link only checks the name; a wrong parameter type, a missing parameter or a
//! `*mut this` on a const method would link and then corrupt the stack or
//! the object. MSVC mangling encodes the full C++ signature, so this decodes
//! it and compares it with the Rust types.
//!
//! The decoder covers the part of MSVC mangling eqlib's methods use so far.
//! Anything else fails with "can't decode", and the decoder needs teaching.

use mq_emu_sys::eqlib::{EQLIB_CPP_FUNCTIONS, EQLIB_METHODS};

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

/// What a function belongs to, which decides how it's called.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Scope {
    /// A non-static member: `__thiscall`, with `this` (const or not) first.
    Member { const_this: bool },
    /// A static member: `__cdecl`.
    Static,
    /// Not a member: `__cdecl`.
    Free,
}

/// Which `imports!` list a declaration is in.
#[derive(Debug, Clone, Copy, PartialEq)]
enum List {
    Methods,
    CppFunctions,
}

/// A decoded function.
#[derive(Debug, PartialEq)]
struct Method {
    /// The class (the namespace for a free function; empty without one).
    class: String,
    scope: Scope,
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
        if self.eat("?$") {
            let name = self.template_name()?;
            if self.names.len() < 10 {
                self.names.push(name.clone());
            }
            return Ok(name);
        }
        if self.rest.starts_with('?') {
            return self.fail("special name");
        }
        let end = self.rest.find('@').ok_or("unterminated name")?;
        let name = self.rest[..end].to_owned();
        self.rest = &self.rest[end + 1..];
        if self.names.len() < 10 {
            self.names.push(name.clone());
        }
        Ok(name)
    }

    /// A template instance's name after `?$` (`VePointer@VItemClient@eqlib@@@`),
    /// as the layouts name it (`VePointer_ItemClient`). Its arguments have
    /// their own name back-references, starting with the template's name.
    /// Only class arguments are taken.
    fn template_name(&mut self) -> Result<String> {
        let end = self.rest.find('@').ok_or("unterminated template name")?;
        let mut name = self.rest[..end].to_owned();
        let mut inner = Decoder {
            rest: &self.rest[end + 1..],
            names: vec![name.clone()],
            param_types: Vec::new(),
        };
        while !inner.eat("@") {
            match inner.ty()? {
                Type::Class(arg) => {
                    name.push('_');
                    name.push_str(&arg);
                }
                other => return Err(format!("can't decode template argument {other:?}")),
            }
        }
        self.rest = inner.rest;
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
        fragments.remove(0); // the function's own name
    }
    let class = fragments.first().cloned().unwrap_or_default();
    let scope = match d.next()? {
        'Q' | 'A' | 'I' | 'U' | 'E' | 'M' => Scope::Member {
            const_this: match d.next()? {
                'A' => false,
                'B' => true,
                _ => return d.fail("`this` qualifier"),
            },
        },
        'S' | 'C' | 'K' => Scope::Static,
        'Y' => Scope::Free,
        _ => return d.fail("access"),
    };
    let convention = d.next()?;
    match scope {
        Scope::Member { .. } if convention != 'E' => return Err("not __thiscall".to_owned()),
        Scope::Static | Scope::Free if convention != 'A' => return Err("not __cdecl".to_owned()),
        _ => {}
    }
    if class.is_empty() && scope != Scope::Free {
        return d.fail("member without a class");
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
        scope,
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

/// Compares a declaration in `list` with its mangled name; returns what's wrong.
fn check(list: List, mangled: &str, args: &str, ret: &str) -> Vec<String> {
    let method = match decode(mangled) {
        Ok(method) => method,
        Err(e) => return vec![e],
    };
    match (list, method.scope) {
        (List::Methods, Scope::Static) => return vec!["a static member function: declare it under `cpp_functions`, not `methods`".to_owned()],
        (List::Methods, Scope::Free) => return vec!["a free function: declare it under `cpp_functions`, not `methods`".to_owned()],
        (List::CppFunctions, Scope::Member { .. }) => return vec!["a non-static member function: declare it under `methods`, not `cpp_functions`".to_owned()],
        _ => {}
    }
    let mut problems = Vec::new();
    let mismatch = |what: String, actual: &str, ty: &Type| {
        let forms = ty.rust_forms();
        (!forms.iter().any(|f| f == actual)).then(|| format!("{what} is `{actual}`, C++ has {}", forms.join(" or ")))
    };
    let all = rust_params(args);
    let (params, first, after) = match method.scope {
        Scope::Member { const_this } => {
            let Some((this, params)) = all.split_first() else {
                return vec!["no `this` parameter".to_owned()];
            };
            let this_type = Type::Pointer {
                is_const: const_this,
                to: Box::new(Type::Class(method.class.clone())),
            };
            problems.extend(mismatch("`this`".to_owned(), this, &this_type));
            (params, 2, " after `this`")
        }
        Scope::Static | Scope::Free => (&all[..], 1, ""),
    };
    // A class returned by value comes back through a hidden pointer the caller
    // passes first (after `this`), which the function returns in EAX: Rust
    // declares both.
    let (params, first, returns) = match &method.returns {
        Some(Type::Class(class)) => {
            let out = Type::Pointer { is_const: false, to: Box::new(Type::Class(class.clone())) };
            let Some((out_param, params)) = params.split_first() else {
                return vec![format!("no out parameter for the {class} C++ returns by value")];
            };
            problems.extend(mismatch(format!("parameter {first} (the out parameter for the {class} returned by value)"), out_param, &out));
            (params, first + 1, Some(out))
        }
        returns => (params, first, returns.clone()),
    };
    if params.len() != method.params.len() {
        problems.push(format!("{} parameters{after}, C++ has {}", params.len(), method.params.len()));
    } else {
        for (i, (actual, ty)) in params.iter().zip(&method.params).enumerate() {
            problems.extend(mismatch(format!("parameter {}", i + first), actual, ty));
        }
    }
    let ret = normalize(&tokens(ret));
    match &returns {
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
fn every_eqlib_method_and_cpp_function_matches_its_mangled_signature() {
    let lists = [(List::Methods, EQLIB_METHODS), (List::CppFunctions, EQLIB_CPP_FUNCTIONS)];
    let failures: Vec<String> = lists
        .iter()
        .flat_map(|&(list, declared)| declared.iter().map(move |&(mangled, args, ret)| (list, mangled, args, ret)))
        .filter_map(|(list, mangled, args, ret)| {
            let problems = check(list, mangled, args, ret);
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
            scope: Scope::Member { const_this: true },
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
    let problems = check(List::Methods, "?Max_HP@CharacterZoneClient@eqlib@@QAEHH_N@Z", "this: *mut CharacterZoneClient, spawn_type: c_int, cap_at_max: bool", "c_int");
    assert!(problems.is_empty(), "{problems:?}");
}

// The checks below prove the test catches each kind of mistake.

#[test]
fn rejects_a_wrong_parameter_type() {
    let problems = check(List::Methods, "?Max_HP@CharacterZoneClient@eqlib@@QAEHH_N@Z", "this: *mut CharacterZoneClient, spawn_type: c_int, cap_at_max: c_int", "c_int");
    assert_eq!(problems, ["parameter 3 is `i32`, C++ has bool"]);
}

#[test]
fn rejects_a_missing_parameter() {
    let problems = check(List::Methods, "?Max_HP@CharacterZoneClient@eqlib@@QAEHH_N@Z", "this: *mut CharacterZoneClient, spawn_type: c_int", "c_int");
    assert_eq!(problems, ["1 parameters after `this`, C++ has 2"]);
}

#[test]
fn rejects_a_mutable_this_on_a_const_method() {
    let problems = check(List::Methods, "?GetType@CXWnd@eqlib@@QBE?AW4UIType@2@XZ", "this: *mut CXWnd", "c_int");
    assert_eq!(problems, ["`this` is `* mut CXWnd`, C++ has * const CXWnd"]);
}

#[test]
fn rejects_the_wrong_class() {
    let problems = check(List::Methods, "?GetType@CXWnd@eqlib@@QBE?AW4UIType@2@XZ", "this: *const CSidlScreenWnd", "c_int");
    assert_eq!(problems, ["`this` is `* const CSidlScreenWnd`, C++ has * const CXWnd"]);
}

#[test]
fn rejects_a_wrong_return_type() {
    let problems = check(List::Methods, "??1CXStr@eqlib@@QAE@XZ", "this: *mut CXStr", "bool");
    assert_eq!(problems, ["returns `bool`, C++ returns nothing"]);
    let problems = check(List::Methods, "?Max_HP@CharacterZoneClient@eqlib@@QAEHH_N@Z", "this: *mut CharacterZoneClient, spawn_type: c_int, cap_at_max: bool", "");
    assert_eq!(problems, ["the return type is ``, C++ has i32"]);
}

#[test]
fn a_class_returned_by_value_is_an_out_parameter_and_the_return() {
    let mangled = "?CleanItemTags@eqlib@@YA?AVCXStr@1@ABV21@_N@Z";
    assert_eq!(check(List::CppFunctions, mangled, "out: *mut CXStr, text: *const CXStr, flag: bool", "*mut CXStr"), Vec::<String>::new());
    assert_eq!(
        check(List::CppFunctions, mangled, "text: *const CXStr, flag: bool", "*mut CXStr"),
        ["parameter 1 (the out parameter for the CXStr returned by value) is `* const CXStr`, C++ has * mut CXStr", "1 parameters, C++ has 2"]
    );
    assert_eq!(check(List::CppFunctions, mangled, "out: *mut CXStr, text: *const CXStr, flag: bool", ""), ["the return type is ``, C++ has * mut CXStr"]);
    assert_eq!(check(List::CppFunctions, mangled, "", "*mut CXStr"), ["no out parameter for the CXStr C++ returns by value"]);
}

#[test]
fn decodes_template_names_as_the_layouts_name_them() {
    let mangled = "?GetItemByGlobalIndex@CharacterBase@eqlib@@QBE?AV?$VePointer@VItemClient@eqlib@@@2@ABVItemGlobalIndex@2@@Z";
    let method = decode(mangled).unwrap();
    assert_eq!(method.returns, Some(Type::Class("VePointer_ItemClient".into())));
    // `2` after the template is still the outer `eqlib`.
    assert_eq!(method.params, [Type::Reference { is_const: true, to: Box::new(Type::Class("ItemGlobalIndex".into())) }]);
    let args = "this: *const CharacterBase, out: *mut VePointer_ItemClient, location: *const ItemGlobalIndex";
    assert_eq!(check(List::Methods, mangled, args, "*mut VePointer_ItemClient"), Vec::<String>::new());
    assert_eq!(check(List::Methods, mangled, args, "*mut VePointer_PcClient"), ["the return type is `* mut VePointer_PcClient`, C++ has * mut VePointer_ItemClient"]);
    // A template argument that isn't a class isn't taken.
    assert!(decode("?Get@?$Array@H@eqlib@@QAEXXZ").unwrap_err().starts_with("can't decode template argument"));
}

#[test]
fn decodes_static_members_and_free_functions() {
    let instance = decode("?Instance@AchievementManager@eqlib@@SAAAV12@XZ").unwrap();
    assert_eq!(instance.scope, Scope::Static);
    let manager = Type::Class("AchievementManager".into());
    assert_eq!(instance.returns, Some(Type::Reference { is_const: false, to: Box::new(manager) }));
    let free = decode("?GetSpellByID@eqlib@@YAPAVEQ_Spell@1@H@Z").unwrap();
    assert_eq!(free.scope, Scope::Free);
    assert_eq!(free.params, [Type::Prim("i32")]);
    assert_eq!(decode("?Count@@YAHXZ").unwrap().class, "");
}

#[test]
fn accepts_a_matching_cpp_function() {
    let problems = check(List::CppFunctions, "?Instance@AchievementManager@eqlib@@SAAAV12@XZ", "", "*mut AchievementManager");
    assert!(problems.is_empty(), "{problems:?}");
    let problems = check(List::CppFunctions, "?GetSpellByID@eqlib@@YAPAVEQ_Spell@1@H@Z", "id: c_int", "*mut EQ_Spell");
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn rejects_a_function_in_the_wrong_list() {
    assert_eq!(check(List::Methods, "?Get@CXStr@eqlib@@SAHXZ", "this: *mut CXStr", ""), ["a static member function: declare it under `cpp_functions`, not `methods`"]);
    assert_eq!(check(List::Methods, "?Get@eqlib@@YAHXZ", "this: *mut CXStr", ""), ["a free function: declare it under `cpp_functions`, not `methods`"]);
    assert_eq!(check(List::CppFunctions, "?GetType@CXWnd@eqlib@@QBE?AW4UIType@2@XZ", "", "c_int"), ["a non-static member function: declare it under `methods`, not `cpp_functions`"]);
}

#[test]
fn rejects_a_cpp_function_with_a_wrong_parameter() {
    assert_eq!(check(List::CppFunctions, "?GetSpellByID@eqlib@@YAPAVEQ_Spell@1@H@Z", "id: bool", "*mut EQ_Spell"), ["parameter 1 is `bool`, C++ has i32"]);
    assert_eq!(check(List::CppFunctions, "?GetSpellByID@eqlib@@YAPAVEQ_Spell@1@H@Z", "", "*mut EQ_Spell"), ["0 parameters, C++ has 1"]);
}

#[test]
fn rejects_another_calling_convention() {
    // A __stdcall (`G`) static member.
    assert_eq!(check(List::CppFunctions, "?Get@CXStr@eqlib@@SGHXZ", "", "c_int"), ["not __cdecl"]);
}

#[test]
fn rejects_what_it_cant_decode() {
    let problems = check(List::Methods, "?Get@CXWnd@eqlib@@QAEX$$QAVCXStr@2@@Z", "this: *mut CXWnd, s: *mut CXStr", "");
    assert_eq!(problems.len(), 1);
    assert!(problems[0].starts_with("can't decode"), "{problems:?}");
}
